#!/bin/bash
set -e

cd "$(dirname "$0")"

# Allow overriding registry and tag via environment variables
REGISTRY=${REGISTRY:-"docker.io/myrepo"}
TAG=${TAG:-"latest"}

echo "==== Building Backend Server ===="
docker build -t $REGISTRY/real-estate-backend:$TAG -f crates/services/server/Dockerfile .

if [ -f "crates/services/scheduler/Dockerfile" ]; then
    echo "==== Building Scheduler Service ===="
    docker build -t $REGISTRY/real-estate-scheduler:$TAG -f crates/services/scheduler/Dockerfile .
fi

if [ -f "crates/services/worker/Dockerfile" ]; then
    echo "==== Building Worker Service ===="
    docker build -t $REGISTRY/real-estate-worker:$TAG -f crates/services/worker/Dockerfile .
fi

echo "==== Building Web Server ===="
docker build -t $REGISTRY/real-estate-web:$TAG -f apps/web/Dockerfile .

# If the Python LLM logic microservice directory exists, build it
if [ -d "apps/llm-logic" ] || [ -f "apps/llm-logic/Dockerfile" ]; then
    echo "==== Building Python LLM Logic ===="
    docker build -t $REGISTRY/real-estate-llm-logic:$TAG -f apps/llm-logic/Dockerfile ./apps/llm-logic
fi

if [ "$1" == "--push" ]; then
    echo "==== Pushing Images to Kubernetes Registry ($REGISTRY) ===="
    docker push $REGISTRY/real-estate-backend:$TAG
    if [ -f "crates/services/scheduler/Dockerfile" ]; then
        docker push $REGISTRY/real-estate-scheduler:$TAG
    fi
    if [ -f "crates/services/worker/Dockerfile" ]; then
        docker push $REGISTRY/real-estate-worker:$TAG
    fi
    docker push $REGISTRY/real-estate-web:$TAG
    if [ -d "apps/llm-logic" ]; then
        docker push $REGISTRY/real-estate-llm-logic:$TAG
    fi
    echo "==== Push Completed ===="
fi

echo "==== All builds completed successfully! ===="
