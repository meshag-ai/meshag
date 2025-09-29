# Makefile for Meshag Infrastructure Services
# Run NATS and Valkey separately for development

ifeq (,$(wildcard ./.env))
    $(warning .env file not found. Environment variables might be missing.)
else
    include ./.env
    export
endif

.PHONY: nats valkey infra up load-env transport stt llm tts all

up:
	cd docker && docker-compose up --build

load-env:
	@powershell -Command "if (Test-Path '.env') { Get-Content '.env' | Where-Object { $$_ -notmatch '^#' -and $$_ -ne '' } | ForEach-Object { $$key, $$value = $$_ -split '=', 2; [Environment]::SetEnvironmentVariable($$key, $$value, 'Process') }; Write-Host 'Environment variables loaded successfully' } else { Write-Host 'Error: .env file not found'; exit 1 }"

# Start NATS JetStream server
nats:
	@echo "Starting NATS JetStream server..."
	docker run -d \
		--name meshag-nats \
		--restart unless-stopped \
		-p 4222:4222 \
		-p 8222:8222 \
		-p 6222:6222 \
		-v nats_data:/data \
		nats:2.10-alpine \
		--jetstream \
		--store_dir=/data \
		--http_port=8222
	@echo "NATS server started on ports 4222 (client), 8222 (monitoring), 6222 (routing)"
	@echo "Monitor at: http://localhost:8222"

nats-ui:
	@echo "Starting NATS UI server..."
	docker run -d \
  		--name meshag-nats-ui \
  		--restart unless-stopped \
  		-p 31311:31311 \
  		-v nats_data:/db \
  		-e NATS_URL=nats://host.docker.internal:4222 \
  		ghcr.io/nats-nui/nui

# Start Valkey server
valkey:
	@echo "Starting Valkey server..."
	docker run -d \
		--name meshag-valkey \
		--restart unless-stopped \
		-p 6379:6379 \
		-v valkey_data:/data \
		valkey/valkey:7.2-alpine \
		valkey-server --appendonly yes
	@echo "Valkey server started on port 6379"

# Start both infrastructure services
infra: nats valkey
	@echo "Infrastructure services started!"
	@echo "NATS: nats://localhost:4222"
	@echo "Valkey: redis://localhost:6379"
	@echo "NATS Monitor: http://localhost:8222"


transport: load-env
	@echo "Starting transport service..."
	cargo run -p transport-service

t: transport

stt: load-env
	@echo "Starting STT service..."
	cargo run -p stt-service

s: stt

llm: load-env
	@echo "Starting LLM service..."
	cargo run -p llm-service

l: llm

tts: load-env
	@echo "Starting TTS service..."
	cargo run -p tts-service

tt: tts

all: load-env
	@echo "Starting all services..."
	cargo run -p transport-service
	cargo run -p stt-service
	cargo run -p llm-service
	cargo run -p tts-service
