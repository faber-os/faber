import { createHttp, type CoreHttp } from './http';
import { CoreSocket, wsUrl } from './socket';
import { wireFormatFromSearch, type WireFormat } from './wire';

export { CoreError, createHttp, type CoreHttp } from './http';
export {
  CoreSocket,
  type ConnectionStatus,
  type SerialHandlers,
  type SerialSession,
} from './socket';
export type { WireFormat } from './wire';

export interface CoreClient {
  http: CoreHttp;
  socket: CoreSocket;
  format: WireFormat;
}

/** Everything needed to talk to the core that served this page. */
export function connectToCore(location: Location): CoreClient {
  const format = wireFormatFromSearch(location.search);
  return {
    http: createHttp(),
    socket: new CoreSocket({ url: wsUrl(location, format), format }),
    format,
  };
}
