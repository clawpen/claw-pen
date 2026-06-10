FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY claw-pen-agent /usr/local/bin/claw-pen-agent

EXPOSE 18790

ENV PORT=18790
ENV LLM_MODEL=kimi-k2.6
ENV LLM_BASE_URL=https://agent-gw.kimi.com/coding/v1

ENTRYPOINT ["/usr/local/bin/claw-pen-agent"]
