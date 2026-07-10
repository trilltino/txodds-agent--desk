# CoralOS runtime participant for proof-guard-agent.
# Build from repo root:
#   docker build -f coral-agents/proof-guard-agent/Dockerfile -t proof-guard-agent:0.1.0 .

FROM python:3.13-slim

WORKDIR /app
RUN pip install --no-cache-dir "mcp>=1.0.0"

COPY coral-agents/proof-guard-agent/agent.py /app/agent.py

ENTRYPOINT ["python", "/app/agent.py"]
