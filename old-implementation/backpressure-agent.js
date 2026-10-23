import { EventEmitter } from 'events';

export class BackpressureAgent extends EventEmitter {
  constructor(serviceId, metricsCollector, config = {}) {
    super();
    this.id = serviceId;
    this.metrics = metricsCollector;
    
    
    this.minLimit = config.minLimit || 1;
    this.maxLimit = config.maxLimit || 100;
    this.limit = this.maxLimit; 


    const baseSeed = config.seed || 42;
    const serviceHash = this._hashCode(this.id);
    this.random = this._createSeededRandom(baseSeed + serviceHash);


    this.clockSkew = process.env.SIMULATE_CLOCK_SKEW === 'true' ? (this.random() * 1000 - 500) : 0;

    
    this.neighborStrains = new Map();
    
    
    this.interval = setInterval(() => this.controlLoop(), 1000);
  }

  stop() {
    if (this.interval) clearInterval(this.interval);
    this.interval = null;
  }

  
  controlLoop() {
    
    const localStrain = this.calculateLocalStrain();
    
    
    const maxNeighborStrain = this.getMaxNeighborStrain();


    const effectiveStrain = Math.max(localStrain, maxNeighborStrain);

    
    this.adjustConcurrency(effectiveStrain);

    
    this.currentGossip = {
      id: this.id,
      strain: localStrain, 
      timestamp: Date.now() + this.clockSkew 
    };
  }

  calculateLocalStrain() {
    const snapshot = this.metrics.getSnapshot();


    const latencyScore = Math.min(snapshot.avgLatency / 1000, 1.0);
    
    
    const errorScore = snapshot.errorRate > 0 ? 1.0 : 0.0;


    return Math.max(latencyScore, errorScore);
  }

  getMaxNeighborStrain() {
    let max = 0;
    for (const strain of this.neighborStrains.values()) {
      if (strain > max) max = strain;
    }
    return max;
  }

  adjustConcurrency(strain) {
    
    const HIGH_STRAIN = 0.7; 
    const MODERATE_STRAIN = 0.4; 

    if (strain > HIGH_STRAIN) {
      
      
      this.limit = Math.max(this.minLimit, Math.floor(this.limit * 0.8));
      
    } else if (strain < MODERATE_STRAIN) {
      
      if (this.limit < this.maxLimit) {
        this.limit += 1;
        
      }
    }
    
  }


  async callDownstreams(downstreams) {
    
    
    const acceptanceProbability = this.limit / this.maxLimit;
    
    if (this.random() > acceptanceProbability) {
      throw new Error(`Backpressure: Throttled by agent (Limit: ${this.limit}/${this.maxLimit})`);
    }

    
    return undefined; 


  }

  handleGossip(payload) {
    if (payload && payload.id && typeof payload.strain === 'number') {
      this.neighborStrains.set(payload.id, payload.strain);
    }
  }


  _hashCode(str) {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
      hash = Math.imul(31, hash) + str.charCodeAt(i) | 0;
    }
    return hash;
  }

  _createSeededRandom(seed) {
    
    return function() {
      let t = seed += 0x6D2B79F5;
      t = Math.imul(t ^ t >>> 15, t | 1);
      t ^= t + Math.imul(t ^ t >>> 7, t | 61);
      return ((t ^ t >>> 14) >>> 0) / 4294967296;
    }
  }
}


export class BackpressureStrategy {
  constructor(config) {
    
    this.agent = new BackpressureAgent(config.serviceId, config.metricsCollector, config);
  }

  start() {
    
  }

  stop() {
    this.agent.stop();
  }

  getGossipReceiver() { return (payload) => this.agent.handleGossip(payload); }
  callDownstreams(downstreams) { return this.agent.callDownstreams(downstreams); }
}