#!/usr/bin/env node


import { writeFileSync, readFileSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

const SCALE    = parseInt(process.argv[2] || '20', 10);
const FAULT    = process.argv[3] || 'baseline-flaky';
const STRATEGY = process.argv[4] || 'none';

const BASE_PORT = 4000;
const NAMESPACE = 'chaos-experiment';
const IMAGE     = 'chaos-experiment:latest';


function generateTopology(n) {
  if (n < 5) throw new Error('SCALE must be >= 5 (1 gateway + 4 tier-1 nodes)');

  const TIER1_IDS = ['svc-a', 'svc-b', 'svc-c', 'svc-d'];
  const t2Count   = n - 1 - 4;

  const services = [{ id: 'gateway', role: 'entry' }];
  const edges    = [];

  for (const id of TIER1_IDS) {
    services.push({ id });
    edges.push({ from: 'gateway', to: id });
  }

  for (let i = 1; i <= t2Count; i++) {
    const id     = `svc-${i}`;
    const parent = TIER1_IDS[(i - 1) % 4];
    services.push({ id });
    edges.push({ from: parent, to: id });
  }

  return {
    name:        `scale-${n}`,
    description: `${n}-node layered topology: 1 gateway + 4 mid-tier (svc-a/b/c/d) + ${t2Count} leaf nodes. Gossip evaluation at scale (N>=${n}).`,
    defaults:    { processingDelayMs: { distribution: 'normal', mean: 20, std: 5 } },
    services,
    edges,
  };
}


function indentLines(str, spaces) {
  const pad = ' '.repeat(spaces);
  return str.split('\n').map(l => pad + l).join('\n');
}


function generateManifests(topology, fault, strategy) {
  const parts = [];

  
  parts.push(`apiVersion: v1
kind: Namespace
metadata:
  name: ${NAMESPACE}
  labels:
    experiment: chaos
    strategy: "${strategy}"
    fault: "${fault}"`);


  const topoJson    = JSON.stringify(topology, null, 2);
  const topoIndented = indentLines(topoJson, 4);

  parts.push(`apiVersion: v1
kind: ConfigMap
metadata:
  name: topology
  namespace: ${NAMESPACE}
data:
  ${topology.name}.json: |
${topoIndented}`);


  let faultJson = '{}';
  try {
    faultJson = readFileSync(resolve(__dirname, 'config', 'faults', `${fault}.json`), 'utf8');
  } catch {
    console.warn(`[generate] Warning: could not read config/faults/${fault}.json — using empty fault config`);
  }
  const faultIndented = indentLines(faultJson, 4);

  parts.push(`apiVersion: v1
kind: ConfigMap
metadata:
  name: fault-profile
  namespace: ${NAMESPACE}
data:
  ${fault}.json: |
${faultIndented}`);


  topology.services.forEach((svc, index) => {
    const port = BASE_PORT + index;
    const name = svc.id; 

    
    parts.push(`apiVersion: v1
kind: Service
metadata:
  name: ${name}
  namespace: ${NAMESPACE}
spec:
  selector:
    app: ${name}
  ports:
    - port: ${port}
      targetPort: ${port}`);


    parts.push(`apiVersion: apps/v1
kind: Deployment
metadata:
  name: ${name}
  namespace: ${NAMESPACE}
spec:
  replicas: 1
  selector:
    matchLabels:
      app: ${name}
  template:
    metadata:
      labels:
        app: ${name}
    spec:
      containers:
        - name: service
          image: ${IMAGE}
          imagePullPolicy: Never
          command: ["node", "runner/service-runner.js"]
          env:
            - name: SERVICE_ID
              value: "${svc.id}"
            - name: TOPOLOGY
              value: "${topology.name}"
            - name: FAULT_PROFILE
              value: "${fault}"
            - name: STRATEGY
              value: "${strategy}"
            - name: SEED
              value: "42"
            - name: BASE_PORT
              value: "${BASE_PORT}"
          ports:
            - containerPort: ${port}
          securityContext:
            capabilities:
              add: ["NET_ADMIN"]
          volumeMounts:
            - name: topology-vol
              mountPath: /app/config/topologies
            - name: fault-vol
              mountPath: /app/config/faults
      volumes:
        - name: topology-vol
          configMap:
            name: topology
        - name: fault-vol
          configMap:
            name: fault-profile`);
  });


  parts.push(`apiVersion: batch/v1
kind: Job
metadata:
  name: coordinator
  namespace: ${NAMESPACE}
spec:
  backoffLimit: 0
  ttlSecondsAfterFinished: 120
  template:
    spec:
      restartPolicy: Never
      containers:
        - name: coordinator
          image: ${IMAGE}
          imagePullPolicy: Never
          command: ["node", "runner/coordinator.js"]
          env:
            - name: TOPOLOGY
              value: "${topology.name}"
            - name: FAULT_PROFILE
              value: "${fault}"
            - name: STRATEGY
              value: "${strategy}"
            - name: SEED
              value: "42"
            - name: BASE_PORT
              value: "${BASE_PORT}"
            - name: RPS
              value: "50"
            - name: DURATION_SEC
              value: "60"
            - name: WARMUP_SEC
              value: "10"
            - name: RESULTS_DIR
              value: "/results"
            - name: HEALTH_TIMEOUT_MS
              value: "300000"
          volumeMounts:
            - name: topology-vol
              mountPath: /app/config/topologies
            - name: fault-vol
              mountPath: /app/config/faults
            - name: results-vol
              mountPath: /results
      volumes:
        - name: topology-vol
          configMap:
            name: topology
        - name: fault-vol
          configMap:
            name: fault-profile
        - name: results-vol
          hostPath:
            path: /results-host
            type: DirectoryOrCreate`);

  return parts.join('\n\n---\n\n');
}


const topology = generateTopology(SCALE);


mkdirSync('config/topologies', { recursive: true });
const topoPath = `config/topologies/${topology.name}.json`;
writeFileSync(topoPath, JSON.stringify(topology, null, 2));
console.log(`[generate] ${topoPath}  (${topology.services.length} nodes, ${topology.edges.length} edges)`);


mkdirSync('k8s', { recursive: true });
const manifests    = generateManifests(topology, FAULT, STRATEGY);
const manifestPath = `k8s/manifests-${FAULT}-${STRATEGY}.yaml`;
writeFileSync(manifestPath, manifests);
console.log(`[generate] ${manifestPath}  (fault=${FAULT} strategy=${STRATEGY})`);
