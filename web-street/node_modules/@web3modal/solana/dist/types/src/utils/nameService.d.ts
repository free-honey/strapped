/// <reference types="node" />
import { PublicKey } from '@solana/web3.js';
import type { Schema } from 'borsh';
export declare class NameRegistry {
    parentName: PublicKey;
    owner: PublicKey;
    class: PublicKey;
    data: Buffer | undefined;
    constructor(obj: {
        parentName: Uint8Array;
        owner: Uint8Array;
        class: Uint8Array;
    });
    static schema: Schema;
}
