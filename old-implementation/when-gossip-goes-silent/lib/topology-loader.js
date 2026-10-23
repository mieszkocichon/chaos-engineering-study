import { readFileSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const CONFIG_DIR = resolve(__dirname, '..', 'config');

export function loadTopology(topologyName, { basePort = 4000, hostResolver = null } = {}) {
  const filePath = resolve(CONFIG_DIR, 'topologies', `${topologyName}.json`);
  const topology = JSON.parse(readFileSync(filePath, 'utf-8'));


  const resolveHost = (typeof hostResolver === 'function') ? hostResolver : () => 'localhost';

  const services = [];
  const portMap = new Map();


  const FETCH_BLOCKED_PORTS = new Set([
    4045,  
    3659,  
    5060, 5061, 
  ]);

  
  let nextPort = basePort;
  topology.services.forEach((svc) => {
    while (FETCH_BLOCKED_PORTS.has(nextPort)) nextPort++;
    portMap.set(svc.id, nextPort++);
  });

  
  for (const svc of topology.services) {
    const port = portMap.get(svc.id);


    const downstreams = topology.edges
      .filter(e => e.from === svc.id && e.kind !== 'gossip')
      .map(e => ({
        id: e.to,
        url: `http://${resolveHost(e.to)}:${portMap.get(e.to)}/data`,
      }));

    
    const neighborIds = new Set();
    for (const e of topology.edges) {
      if (e.from === svc.id) neighborIds.add(e.to);
      if (e.to === svc.id) neighborIds.add(e.from);
    }
    const neighbors = [...neighborIds].map(id => ({
      id,
      gossipUrl: `http://${resolveHost(id)}:${portMap.get(id)}/gossip`,
      metricsUrl: `http://${resolveHost(id)}:${portMap.get(id)}/metrics`,
      controlUrl: `http://${resolveHost(id)}:${portMap.get(id)}/control`,
    }));

    services.push({
      id: svc.id,
      port,
      role: svc.role || 'service',
      downstreams,
      neighbors,
      processingDelay: svc.processingDelay || topology.defaults?.processingDelayMs || { distribution: 'normal', mean: 50, std: 10 },
    });
  }

  const entryPoint = services.find(s => s.role === 'entry');

  return {
    name: topology.name,
    description: topology.description,
    services,
    edges: topology.edges,
    entryPoint: entryPoint ? {
      id: entryPoint.id,
      url: `http://${resolveHost(entryPoint.id)}:${entryPoint.port}/data`,
      port: entryPoint.port,
    } : null,
    portMap,
  };
}

export function loadFaultProfile(faultName) {
  const filePath = resolve(CONFIG_DIR, 'faults', `${faultName}.json`);
  return JSON.parse(readFileSync(filePath, 'utf-8'));
}

export function loadExperimentMatrix(matrixName) {
  const filePath = resolve(CONFIG_DIR, 'experiments', `${matrixName}.json`);
  return JSON.parse(readFileSync(filePath, 'utf-8'));
}
