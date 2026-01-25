FROM node:22-alpine AS build
WORKDIR /app
RUN npm install -g pnpm

# Copy package files for all workspaces
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY frontend/packages/website/package.json ./frontend/packages/website/

# Install dependencies
RUN pnpm install --filter @sandbox-agent/website...

# Copy website source
COPY frontend/packages/website ./frontend/packages/website

# Build
RUN pnpm --filter @sandbox-agent/website build

FROM caddy:alpine
COPY --from=build /app/frontend/packages/website/dist /srv
RUN cat > /etc/caddy/Caddyfile <<'EOF'
:80 {
	root * /srv
	file_server
	try_files {path} /index.html
}
EOF
EXPOSE 80
