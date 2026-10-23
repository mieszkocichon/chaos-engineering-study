//! T041: seedrandom port vs Node (SC-002).

mod common;
use chaos::prng::Prng;
use common::*;

#[test]
fn prng_sequences_match_seedrandom() {
    let g = golden("prng.json");
    let seeds = g.as_array().unwrap();
    assert!(seeds.len() >= 20);
    for s in seeds {
        let seed = s["seed"].as_str().unwrap();
        let label = format!("seed {seed:?}");

        let mut r = Prng::new(seed);
        let v: Vec<String> = (0..10_000).map(|_| hex(r.random())).collect();
        check_seq(&format!("{label} random"), &s["random"], &v);

        let mut b = Prng::new(seed);
        for (i, e) in s["bernoulli"].as_array().unwrap().iter().enumerate() {
            assert_eq!(b.bernoulli(0.3), e.as_bool().unwrap(), "{label} bernoulli[{i}]");
        }
        let mut u = Prng::new(seed);
        for (i, e) in s["uniform"].as_array().unwrap().iter().enumerate() {
            assert_eq!(u.uniform(10.0, 100.0), num(e), "{label} uniform[{i}]");
        }
        let mut n = Prng::new(seed);
        let v: Vec<String> = (0..1000).map(|_| hex(n.normal_random(50.0, 10.0))).collect();
        check_seq(&format!("{label} normal"), &s["normal"], &v);
        let mut e = Prng::new(seed);
        let v: Vec<String> = (0..1000).map(|_| hex(e.exponential(0.5))).collect();
        check_seq(&format!("{label} exponential"), &s["exponential"], &v);
    }
}
