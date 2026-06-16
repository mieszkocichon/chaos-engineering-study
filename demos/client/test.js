const http = require('http');

const options = {
  hostname: 'localhost',
  port: 3000,
  path: '/api/combine',
  method: 'GET'
};

const req = http.request(options, (res) => {
  let data = '';
  res.on('data', chunk => data += chunk);
  res.on('end', () => {
    console.log('Response from Gateway:');
    console.log(data);
  });
});

req.on('error', (e) => {
  console.error('Connection error:', e.message);
});

req.end();