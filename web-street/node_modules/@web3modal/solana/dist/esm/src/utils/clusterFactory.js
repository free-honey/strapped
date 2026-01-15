import { proxy } from 'valtio/vanilla';
import { Transaction } from '@solana/web3.js';
import { waitForOpenConnection } from './websocket.js';
import { SolStoreUtil } from './scaffold/index.js';
let socket = undefined;
const listeners = proxy({});
const subIdToReqId = proxy({});
export async function setSocket() {
    const cluster = SolStoreUtil.getCluster();
    const { endpoint } = cluster;
    socket = new WebSocket(endpoint.replace('http', 'ws'));
    await waitForOpenConnection(socket);
    socket.onmessage = ev => {
        const data = JSON.parse(ev.data);
        if (data.id) {
            subIdToReqId[data.result] = data.id;
        }
        if (data.params?.subscription) {
            const listenerIndex = subIdToReqId[data.params.subscription];
            if (typeof listenerIndex !== 'undefined') {
                listeners[listenerIndex]?.callback(data.params.result);
            }
        }
    };
    return socket;
}
export function unregisterListener(id) {
    const { method } = listeners[id];
    const subscriptionId = Number(Object.entries(subIdToReqId).filter(([_, value]) => value === id)[0]?.[0]);
    const unsubscribeMethod = method.replace('Subscribe', 'Unsubscribe');
    if (!socket) {
        throw new Error('Socket not initialized');
    }
    socket.send(JSON.stringify({
        method: unsubscribeMethod,
        params: [subscriptionId],
        jsonrpc: '2.0',
        id: SolStoreUtil.getNewRequestId()
    }));
}
export async function registerListener(method, cParams, callback) {
    if (!socket) {
        await setSocket();
    }
    const id = SolStoreUtil.getNewRequestId();
    if (!socket) {
        throw new Error('Socket not initialized');
    }
    socket.send(JSON.stringify({
        method,
        cParams,
        jsonrpc: '2.0',
        id
    }));
    listeners[id] = { method, callback, id };
    return id;
}
//# sourceMappingURL=clusterFactory.js.map