import { loadTopology, loadFaultProfile } from '../lib/topology-loader.js';
import { createService } from '../lib/generic-service.js';
import { NoneStrategy } from '../lib/strategies/none.js';
import { StaticStrategy } from '../lib/strategies/static.js';
import { MASStrategy } from '../lib/strategies/mas-agent.js';
import { MASSwimStrategy } from '../lib/strategies/mas-agent-swim.js';
import { MASInboundStrategy } from '../lib/strategies/mas-agent-inbound.js';
import { CentralStrategy } from '../lib/strategies/central.js';
import { BackpressureStrategy } from '../agent/backpressure-agent.js';
import { StaticConservativeStrategy } from '../lib/strategies/static-conservative.js';
import { LeafGossipPusher } from '../lib/leaf-gossip-pusher.js';

const SERVICE_ID = process.env.SERVICE_ID;
const TOPOLOGY = process.env.TOPOLOGY || 'fanout';
const FAULT_PROFILE = process.env.FAULT_PROFILE || 'baseline-flaky';
const STRATEGY = process.env.STRATEGY || 'none';
const SEED = parseInt(process.env.SEED || '42', 10);
const BASE_PORT = parseInt(process.env.BASE_PORT || '4000', 10);

if (!SERVICE_ID) {
  console.error('[service-runner] SERVICE_ID env var is required');
  process.exit(1);
}

const dockerHostResolver = (id) => id;

const topology = loadTopology(TOPOLOGY, { basePort: BASE_PORT, hostResolver: dockerHostResolver });
const faultProfile = loadFaultProfile(FAULT_PROFILE);

const svcConfig = topology.services.find(s => s.id === SERVICE_ID);
if (!svcConfig) {
  console.error(`[service-runner] SERVICE_ID="${SERVICE_ID}" not found in topology "${TOPOLOGY}"`);
  console.error(`[service-runner] Available: ${topology.services.map(s => s.id).join(', ')}`);
  process.exit(1);
}


const faultMap = new Map();
for (const fault of (faultProfile.faults || [])) {
  faultMap.set(fault.targetService, fault);
}
const faultForService = faultMap.get(SERVICE_ID) || { type: 'none' };

function createStrategy(svcConfig) {
  if (svcConfig.downstreams.length === 0) return null;

  switch (STRATEGY) {
    case 'none':
      return new NoneStrategy({ timeoutMs: 3000 });
    case 'static': {
      // Non-breaking extension: STATIC_PARAMS env (JSON) pozwala na sensitivity
      // sweep static-konfiguracji bez modyfikacji defaults. Puste = dotychczasowe
      // zachowanie (retry=3, cb=5, conc=50, timeout=3000).
      let staticOpts = {};
      if (process.env.STATIC_PARAMS) {
        try { staticOpts = JSON.parse(process.env.STATIC_PARAMS); }
        catch (e) { console.warn(`[service-runner] bad STATIC_PARAMS: ${e.message}`); }
      }
      return new StaticStrategy(staticOpts);
    }
    case 'central':
      return new CentralStrategy();
    case 'mas-agent':
      return new MASStrategy({
        serviceId: svcConfig.id,
        metricsCollector: null,
        neighbors: svcConfig.neighbors,
      });
    case 'mas-agent-swim':
      return new MASSwimStrategy({
        serviceId: svcConfig.id,
        metricsCollector: null,
        neighbors: svcConfig.neighbors,
        gossipFanout: parseInt(process.env.SWIM_FANOUT || '3', 10),
      });
    case 'mas-agent-inbound':
      return new MASInboundStrategy({
        serviceId: svcConfig.id,
        metricsCollector: null,
        neighbors: svcConfig.neighbors,
      });
    case 'backpressure':
      return new BackpressureStrategy({
        serviceId: svcConfig.id,
        metricsCollector: null,
        neighbors: svcConfig.neighbors,
        seed: SEED,
      });
    case 'static-conservative':
      return new StaticConservativeStrategy();
    default:
      throw new Error(`[service-runner] Unknown strategy: ${STRATEGY}`);
  }
}

const strategy = createStrategy(svcConfig);

let leafGossipPusher = null;
if (!strategy && svcConfig.neighbors.length > 0) {
  leafGossipPusher = new LeafGossipPusher({
    serviceId: svcConfig.id,
    metricsCollector: null,
    upstreamNeighbors: svcConfig.neighbors,
  });
}

const service = createService({
  id: svcConfig.id,
  port: svcConfig.port,
  downstreams: svcConfig.downstreams,
  faultProfile: faultForService,
  seed: SEED,
  processingDelay: svcConfig.processingDelay,
  strategy,
});


if (strategy && strategy.agent) {
  strategy.agent.metricsCollector = service.metrics;
}
if (leafGossipPusher) {
  leafGossipPusher.metricsCollector = service.metrics;
}


if (strategy && strategy.getGossipReceiver) {
  service.onGossip(strategy.getGossipReceiver());
}
if (strategy && strategy.applyUpdate) {
  service.onControl((params) => strategy.applyUpdate(params));
}
if (strategy && strategy.reset) {
  service.onReset(() => strategy.reset());
}


await service.start();
console.log(`[${SERVICE_ID}] Listening on port ${svcConfig.port} | topology=${TOPOLOGY} fault=${FAULT_PROFILE} strategy=${STRATEGY} seed=${SEED}`);


if (strategy && strategy.start) {
  strategy.start();
  console.log(`[${SERVICE_ID}] Strategy "${STRATEGY}" started`);
}
if (leafGossipPusher) {
  leafGossipPusher.start();
  console.log(`[${SERVICE_ID}] LeafGossipPusher started (pushing to ${svcConfig.neighbors.length} neighbors)`);
}


process.on('SIGTERM', () => {
  console.log(`[${SERVICE_ID}] SIGTERM received, shutting down`);
  if (strategy && strategy.stop) strategy.stop();
  if (leafGossipPusher) leafGossipPusher.stop();
  process.exit(0);
});
