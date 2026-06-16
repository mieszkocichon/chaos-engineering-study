import seedrandom from 'seedrandom';

export function createPRNG(seed) {
  const rng = seedrandom(String(seed));

  function random() {
    return rng();
  }

  function bernoulli(p) {
    return random() < p;
  }

  function uniform(min, max) {
    return min + Math.floor(random() * (max - min + 1));
  }

  function normalRandom(mean, std) {
    // Box-Muller transform
    const u1 = random();
    const u2 = random();
    const z = Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
    return mean + z * std;
  }

  function exponential(lambda) {
    return -Math.log(1 - random()) / lambda;
  }

  return { random, bernoulli, uniform, normalRandom, exponential };
}
