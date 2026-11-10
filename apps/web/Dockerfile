# Stage 1: Build
FROM oven/bun:1 AS builder
WORKDIR /app

# Copy root lockfile and package.json files
COPY package.json bun.lock ./
COPY apps/web/package.json ./apps/web/

# Install dependencies
RUN bun install

# Copy all files to /app root
WORKDIR /app
COPY . .

# NEXT_PUBLIC_* values are inlined into the bundle at build time
ARG NEXT_PUBLIC_LIVEKIT_URL
ENV NEXT_PUBLIC_LIVEKIT_URL=$NEXT_PUBLIC_LIVEKIT_URL

ARG NEXT_PUBLIC_GOOGLE_CLIENT_ID
ENV NEXT_PUBLIC_GOOGLE_CLIENT_ID=$NEXT_PUBLIC_GOOGLE_CLIENT_ID

ARG API_URL=http://api:4000
ENV API_URL=$API_URL
ARG BACKEND_URL=http://api:4000
ENV BACKEND_URL=$BACKEND_URL
ARG VOICE_SERVER_URL=http://voice-server:4001
ENV VOICE_SERVER_URL=$VOICE_SERVER_URL

# Build the Next.js application
WORKDIR /app/apps/web
RUN bunx next build

# Stage 2: Runner
FROM oven/bun:1-alpine AS runner
WORKDIR /app

# Copy the built app from the builder stage
COPY --from=builder /app /app

WORKDIR /app/apps/web

EXPOSE 3000
CMD ["bun", "run", "start"]
