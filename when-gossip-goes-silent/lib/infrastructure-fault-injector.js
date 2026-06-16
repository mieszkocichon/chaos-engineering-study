import { exec } from 'child_process';
import util from 'util';

const execPromise = util.promisify(exec);

export class InfrastructureFaultInjector {
  constructor() {
    this.activeFaults = new Set();
    this._degradationTimer = null;
    // Tracks IP addresses currently blocked via iptables (for network-partition cleanup)
    this._blockedIps = new Set();
  }

  async inject(profile, options = {}) {
    console.log(`[InfraFault] Injecting profile: ${profile}`);

    // Only clear existing tc qdisc when we're about to install a new one.
    // network-partition uses iptables (orthogonal to tc), so it stacks on top
    // of any active WAN baseline without clearing it. This matters when a WAN
    // baseline is pre-installed on the gateway and we later overlay partition.
    const PROFILES_USING_TC = new Set([
      'baseline-flaky', 'burst-failure', 'slow-degradation', 'wan-baseline',
    ]);
    if (PROFILES_USING_TC.has(profile)) {
      await this.clearNetworkFaults();
    }

    switch (profile) {

      case 'baseline-flaky':
        await this.runCommand(
          'tc qdisc add dev eth0 root netem loss 40%'
        );
        break;

      case 'burst-failure':
        await this.runCommand(
          'tc qdisc add dev eth0 root netem loss 80%'
        );
        break;

      case 'slow-degradation': {
        const MAX_DELAY_MS = 5000;
        const DURATION_MS = 60_000;
        const STEP_MS = 1_000;
        const steps = DURATION_MS / STEP_MS;
        const delayPerStep = MAX_DELAY_MS / steps;

        let currentDelayMs = 0;
        let stepsDone = 0;

        await this.runCommand(
          'tc qdisc add dev eth0 root netem delay 0ms 10ms distribution normal'
        );

        this._degradationTimer = setInterval(async () => {
          stepsDone++;
          currentDelayMs = Math.min(MAX_DELAY_MS, Math.round(delayPerStep * stepsDone));
          const jitterMs = Math.round(currentDelayMs * 0.1);
          await this.runCommand(
            `tc qdisc replace dev eth0 root netem delay ${currentDelayMs}ms ${jitterMs}ms distribution normal`
          );
          if (stepsDone >= steps) {
            clearInterval(this._degradationTimer);
            this._degradationTimer = null;
          }
        }, STEP_MS);
        break;
      }

      case 'cascade-crash':
        console.log('[InfraFault] Executing SIGKILL on self...');
        process.kill(process.pid, 'SIGKILL');
        break;

      // Simulates constant WAN inter-DC latency as a baseline condition (not a fault event).
      // 50ms ± 10ms (normal distribution) models a typical same-region DC-to-DC link.
      // This makes gossip convergence times realistic: at 3000ms gossip interval with
      // 50ms RTT, each hop costs ~100ms round-trip, exposing propagation lag at N>30.
      case 'wan-baseline': {
        const baseDelayMs = options.delayMs ?? 50;
        const jitterMs = options.jitterMs ?? 10;
        await this.runCommand(
          `tc qdisc add dev eth0 root netem delay ${baseDelayMs}ms ${jitterMs}ms distribution normal`
        );
        break;
      }

      // Drops all TCP packets from specific peer IPs using iptables INPUT rules.
      // Models true split-brain: the affected node remains alive and reachable from
      // other nodes, but cannot receive messages from the listed peers — gossip
      // from those peers goes silent while the node keeps gossiping to them.
      // options.peerIps: string[] — list of IP addresses to block (required)
      case 'network-partition': {
        const peerIps = options.peerIps ?? [];
        if (peerIps.length === 0) {
          console.warn('[InfraFault] network-partition: no peerIps provided, no-op');
          break;
        }
        for (const ip of peerIps) {
          await this.runCommand(`iptables -A INPUT -s ${ip} -j DROP`);
          await this.runCommand(`iptables -A OUTPUT -d ${ip} -j DROP`);
          this._blockedIps.add(ip);
        }
        console.log(`[InfraFault] Partitioned from ${peerIps.length} peers: ${peerIps.join(', ')}`);
        break;
      }

      default:
        console.log(`[InfraFault] Unknown profile '${profile}', doing nothing.`);
    }
  }

  async recover() {
    console.log('[InfraFault] Recovering infrastructure...');
    await this.clearNetworkFaults();
    await this.clearNetworkPartition();
  }

  // Type-scoped recovery: clears only the fault layer matching the given type.
  // Used when WAN baseline persists on the same container as a transient overlay
  // (e.g. network-partition on gateway) and only the overlay should be removed.
  async recoverByType(type) {
    switch (type) {
      case 'network-partition':
        await this.clearNetworkPartition();
        break;
      case 'wan-baseline':
      case 'baseline-flaky':
      case 'burst-failure':
      case 'slow-degradation':
        await this.clearNetworkFaults();
        break;
      default:
        console.warn(`[InfraFault] recoverByType: unknown type '${type}', falling back to full recover`);
        await this.recover();
    }
  }

  async clearNetworkFaults() {
    if (this._degradationTimer) {
      clearInterval(this._degradationTimer);
      this._degradationTimer = null;
    }
    try {
      await this.runCommand('tc qdisc del dev eth0 root');
    } catch {
      // no qdisc installed — ignore
    }
  }

  async clearNetworkPartition() {
    for (const ip of this._blockedIps) {
      await this.runCommand(`iptables -D INPUT -s ${ip} -j DROP`).catch(() => {});
      await this.runCommand(`iptables -D OUTPUT -d ${ip} -j DROP`).catch(() => {});
    }
    this._blockedIps.clear();
  }

  async runCommand(cmd) {
    try {
      await execPromise(cmd);
      console.log(`[InfraFault] Executed: ${cmd}`);
    } catch (error) {
      console.error(`[InfraFault] Error executing "${cmd}": ${error.message}`);
    }
  }
}
