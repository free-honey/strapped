import { Transaction } from '@solana/web3.js';
import type { ClusterSubscribeRequestMethods } from './scaffold/index.js';
export declare function setSocket(): Promise<WebSocket>;
export declare function unregisterListener(id: number): void;
export declare function registerListener<Method extends keyof ClusterSubscribeRequestMethods>(method: Method, cParams: ClusterSubscribeRequestMethods[Method]['params'], callback: (params: Transaction | number) => void): Promise<number>;
