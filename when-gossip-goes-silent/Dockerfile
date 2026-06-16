FROM node:22-slim

WORKDIR /app

# Instalacja narzędzi: tc (iproute2), procps, curl + k6 (generator obciążenia)
RUN apt-get update && apt-get install -y iproute2 procps curl iptables && \
    curl -L -o /tmp/k6.tar.gz \
      https://github.com/grafana/k6/releases/download/v0.55.0/k6-v0.55.0-linux-amd64.tar.gz && \
    tar xzf /tmp/k6.tar.gz -C /tmp && \
    mv /tmp/k6-v0.55.0-linux-amd64/k6 /usr/local/bin/k6 && \
    rm -rf /tmp/k6.tar.gz /tmp/k6-v0.55.0-linux-amd64 && \
    rm -rf /var/lib/apt/lists/*

COPY package*.json ./
RUN npm install

COPY . .

CMD ["node", "runner/service-runner.js"]