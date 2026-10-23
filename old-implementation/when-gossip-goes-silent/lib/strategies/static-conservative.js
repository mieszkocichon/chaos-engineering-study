import { StaticStrategy } from './static.js';

export class StaticConservativeStrategy extends StaticStrategy {
  constructor() {
    super({
      retryCount: 1,
      cbThreshold: 15,
      cbTimeoutMs: 5000,
      maxConcurrent: 100,
      retryBaseMs: 500,
    });
  }

  getStats() {
    const s = super.getStats();
    s.strategy = 'static-conservative';
    return s;
  }
}