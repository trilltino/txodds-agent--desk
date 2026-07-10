# CoralOS runtime participant for sharp-movement-detector.
# Build from repo root:
#   docker build -f coral-agents/sharp-movement-detector/Dockerfile -t sharp-movement-detector:0.1.0 .

FROM python:3.13-slim

WORKDIR /app
RUN pip install --no-cache-dir "mcp>=1.0.0"

COPY coral-agents/sharp-movement-detector/agent.py /app/agent.py

ENTRYPOINT ["python", "/app/agent.py"]
