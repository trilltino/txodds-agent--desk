# CoralOS runtime participant for fan-pundit-agent.
# Build from repo root:
#   docker build -f coral-agents/fan-pundit-agent/Dockerfile -t fan-pundit-agent:0.1.0 .

FROM python:3.13-slim

WORKDIR /app
RUN pip install --no-cache-dir "mcp>=1.0.0"

COPY coral-agents/fan-pundit-agent/agent.py /app/agent.py

ENTRYPOINT ["python", "/app/agent.py"]
