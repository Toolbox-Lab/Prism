export {
  getRedis,
  closeRedis,
  cacheSet,
  cacheGet,
  cacheDel,
  acquireLock,
  incrBy,
  decrBy,
  getNumber,
  setExpiry,
  scanKeys,
} from "./client";
