/** Where report files live. `put` returns the path to use from then on; it may differ from the one asked for. */
export interface FileStore {
  put(path: string, bytes: Uint8Array, contentType: string): Promise<string>;
  get(path: string): Promise<Uint8Array | null>;
  del(paths: string[]): Promise<void>;
}
