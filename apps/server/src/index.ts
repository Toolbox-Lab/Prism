import Fastify from "fastify";
import { replayRoutes } from "./routes/replay";
import { healthRoutes } from "./routes/health";
import { sessionRoutes } from "./routes/session";
import { aiRoutes } from "./routes/ai";
import { config } from "./config";
import { SessionStore } from "./session-store";

const server = Fastify({ logger: true });

// ---------------------------------------------------------------------------
// Bootstrap the session store and attach it as a decorator so every route
// plugin can access the same Redis-backed store instance.
// ---------------------------------------------------------------------------
const sessionStore = new SessionStore(config.redisUrl);

server.decorate("sessionStore", sessionStore);

// Ensure the Redis connection is live before accepting traffic, and close it
// cleanly when the process shuts down.
server.addHook("onReady", async () => {
  await sessionStore.connect();
  server.log.info("Session store connected to Redis");
});

server.addHook("onClose", async () => {
  await sessionStore.disconnect();
  server.log.info("Session store disconnected from Redis");
});

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------
server.register(healthRoutes);
server.register(replayRoutes, { prefix: "/api" });
server.register(sessionRoutes, { prefix: "/api" });
server.register(aiRoutes, { prefix: "/api" });

// ---------------------------------------------------------------------------
// Start
// ---------------------------------------------------------------------------
server.listen({ port: config.port, host: "0.0.0.0" }, (err) => {
  if (err) {
    server.log.error(err);
    process.exit(1);
  }
});
