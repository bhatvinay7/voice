# Stage 1: Build
FROM oven/bun:1 AS builder
WORKDIR /app

# Copy root lockfile and package.json files
COPY package.json bun.lock ./
COPY apps/web/package.json ./apps/web/

# Install dependencies
RUN bun install

# Copy all files
COPY . .

# Build the Next.js application
WORKDIR /app/apps/web
RUN bun run build

# Stage 2: Runner
FROM oven/bun:1-alpine AS runner
WORKDIR /app

# Copy the built app from the builder stage
COPY --from=builder /app /app

WORKDIR /app/apps/web

EXPOSE 3000
CMD ["bun", "run", "start"]
