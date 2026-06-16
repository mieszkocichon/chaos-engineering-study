


import http from 'k6/http';
import { check } from 'k6';

const TARGET_URL   = __ENV.TARGET_URL   || 'http://gateway:4000/data';
const RPS          = parseInt(__ENV.RPS          || '50');
const DURATION     = parseInt(__ENV.DURATION_SEC || '60');
const WARMUP       = parseInt(__ENV.WARMUP_SEC   || '10');
const SUMMARY_FILE = __ENV.SUMMARY_FILE || '/results/_k6_summary_tmp.json';


export const options = {
  scenarios: {
    warmup: {
      executor:        'constant-arrival-rate',
      rate:            RPS,
      timeUnit:        '1s',
      duration:        `${WARMUP}s`,
      preAllocatedVUs: 20,
      maxVUs:          60,
      tags:            { phase: 'warmup' },
      gracefulStop:    '3s',
    },
    measurement: {
      executor:        'constant-arrival-rate',
      rate:            RPS,
      timeUnit:        '1s',
      duration:        `${DURATION}s`,
      startTime:       `${WARMUP}s`,
      preAllocatedVUs: 20,
      maxVUs:          60,
      tags:            { phase: 'measurement' },
      gracefulStop:    '3s',
    },
  },

  
  summaryTrendStats: ['avg', 'min', 'med', 'max', 'p(50)', 'p(90)', 'p(99)'],
};

export default function () {
  const res = http.get(TARGET_URL, {
    timeout: '15s',
    tags: { name: 'gateway' },
  });

  check(res, {
    'status 200': (r) => r.status === 200,
  });
}


export function handleSummary(data) {
  return {
    [SUMMARY_FILE]: JSON.stringify(data),
  };
}
