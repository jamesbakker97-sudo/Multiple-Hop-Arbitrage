FROM node:18-alpine AS build
WORKDIR /app
COPY control-plane/package.json control-plane/package-lock.json* ./
RUN npm ci --production=false || true
COPY control-plane/ ./
RUN npm run build --workspace=.

FROM node:18-alpine
WORKDIR /app
COPY --from=build /app/control-plane/package.json ./
COPY --from=build /app/control-plane/dist ./dist
ENV NODE_ENV=production
EXPOSE 3000
CMD ["node", "dist/index.js"]
