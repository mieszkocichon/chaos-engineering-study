import express from 'express';
import { MetricsCollector } from './metrics-collector.js';
import { InfrastructureFaultInjector } from './infrastructure-fault-injector.js';
import { createPRNG } from './prng.js';


export function createService(config) {
  const {
    id,
    port,
    downstreams = [],
    faultProfile = { type: 'none' },
    seed = 'default',
    processingDelay = { distribution: 'normal', mean: 50, std: 10 },
    strategy,
  } = config;

  const app = express();
  app.use(express.json());

  const metrics = new MetricsCollector();

  const faultInjector = new InfrastructureFaultInjector();
  let currentSeed = seed;
  let prng = createPRNG(`${currentSeed}_${id}_delay`);


  let activeStrategy = strategy;

  let controlHandler = null;
  let gossipHandler = null;
  let resetHandler = null;

  function getProcessingDelay() {
    const d = processingDelay;
    if (typeof d === 'number') return d;
    if (d.distribution === 'normal') {
      return Math.max(0, Math.round(prng.normalRandom(d.mean || 50, d.std || 10)));
    }
    if (d.distribution === 'uniform') {
      return prng.uniform(d.min || 10, d.max || 100);
    }
    return d.mean || 50;
  }

  // Soft reset between seeds within a single experiment cell.
  // Restores in-process state so subsequent seed executes as if on a fresh container.
  async function softReset(newSeed) {
    currentSeed = newSeed ?? currentSeed;
    // Re-seed PRNG so processing delays reproduce bit-for-bit for the new seed.
    prng = createPRNG(`${currentSeed}_${id}_delay`);
    metrics.reset();
    // Recover any active network faults so next seed starts with clean eth0/iptables.
    await faultInjector.recover().catch((err) => {
      console.warn(`[${id}] softReset: faultInjector.recover failed: ${err.message}`);
    });
    // Let the strategy reset its internal state (CB window, gossip cache, ODA history).
    if (resetHandler) {
      try { await resetHandler({ seed: currentSeed }); }
      catch (err) { console.warn(`[${id}] softReset: strategy reset failed: ${err.message}`); }
    }
  }


  app.get('/data', async (req, res) => {
    const start = Date.now();
    metrics.startRequest();

    try {
      const baseDelay = getProcessingDelay();
      if (baseDelay > 0) {
        await new Promise(resolve => setTimeout(resolve, baseDelay));
      }

      let downstreamResults = null;
      if (downstreams.length > 0 && activeStrategy?.callDownstreams) {
        downstreamResults = await activeStrategy.callDownstreams(downstreams);
      }

      const latency = Date.now() - start;
      metrics.recordRequest({ latencyMs: latency, success: true });

      res.json({
        service: id,
        latencyMs: latency,
        downstreamResults,
        timestamp: Date.now(),
      });
    } catch (err) {
      const latency = Date.now() - start;
      metrics.recordRequest({ latencyMs: latency, success: false });
      res.status(502).json({ error: err.message, service: id });
    }
  });

  
  app.get('/health', (req, res) => {
    res.json({ status: 'ok', service: id, uptime: process.uptime() });
  });

  
  app.get('/metrics', (req, res) => {
    const mem = process.memoryUsage();
    res.json({
      service: id,
      snapshot: metrics.getSnapshot(),
      resources: {
        memoryMb: Math.round(mem.heapUsed / 1024 / 1024),
        cpuUser: process.cpuUsage().user,
        cpuSystem: process.cpuUsage().system,
        uptime: process.uptime()
      },
      healthSummary: metrics.getHealthSummary(),
      
      
      strategyStats: activeStrategy?.getStats?.() ?? null,
      timestamp: Date.now(),
    });
  });

  app.post('/gossip', (req, res) => {
    if (gossipHandler) {
      gossipHandler(req.body);
    }
    res.json({ ok: true });
  });

  
  app.post('/control', (req, res) => {
    if (controlHandler) {
      controlHandler(req.body);
    }
    res.json({ ok: true });
  });


  app.post('/faults/recover', async (req, res) => {
    await faultInjector.recover();
    res.json({ status: 'recovered' });
  });

  // Fault-type-scoped recovery, used when multiple fault layers coexist on
  // the same container (e.g. WAN baseline on gateway with overlay partition):
  // clearing only the specified layer leaves the other layers intact.
  app.post('/faults/recover/:type', async (req, res) => {
    const { type } = req.params;
    await faultInjector.recoverByType(type);
    res.json({ status: 'recovered', type });
  });

  app.post('/faults/:profile', async (req, res) => {
    const { profile } = req.params;
    await faultInjector.inject(profile, req.body || {});
    res.json({ status: 'injected', profile });
  });


  app.post('/start', (req, res) => {
    const { experimentStartTime } = req.body || {};
    const t = typeof experimentStartTime === 'number' ? experimentStartTime : Date.now();
    res.json({ ok: true, experimentStartTime: t, service: id });
  });

  // Soft reset endpoint: called by coordinator between seeds within a single cell.
  // Body: { seed: <number> } — new seed for next iteration.
  app.post('/experiment/reset', async (req, res) => {
    const { seed: newSeed } = req.body || {};
    await softReset(newSeed);
    res.json({ ok: true, service: id, seed: currentSeed });
  });

  
  function onGossip(handler) {
    gossipHandler = handler;
  }

  function onControl(handler) {
    controlHandler = handler;
  }

  function onReset(handler) {
    resetHandler = handler;
  }

  function setStartTime(t) {
    // intentionally empty — kept for interface compatibility
  }

  function start() {
    return new Promise((resolve) => {
      const server = app.listen(port, () => {
        resolve(server);
      });
    });
  }

  return {
    app,
    metrics,
    faultInjector,
    start,
    setStartTime,
    onGossip,
    onControl,
    onReset,
    config: { id, port, downstreams },
  };
}
