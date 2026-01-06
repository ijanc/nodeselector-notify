# Build the project
build:
    cargo build

# Build release binary
release:
    cargo build --release

# Run linter
lint:
    cargo clippy

# Format code
fmt:
    cargo fmt

# Check formatting
fmt-check:
    cargo fmt --check

# Run locally (requires SLACK_WEBHOOK_URL env var)
run:
    cargo run

# Build Docker image
docker-build tag="nodeselector-notify:latest":
    docker build -t {{tag}} .

# Deploy to Kubernetes
deploy:
    kubectl apply -f k8s/manifests.yaml

# Remove from Kubernetes
undeploy:
    kubectl delete -f k8s/manifests.yaml
