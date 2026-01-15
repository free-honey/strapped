/// <reference types="node" />
import { Buffer } from 'buffer';
import { PublicKey } from '@solana/web3.js';
export declare function getHashedName(name: string): Buffer;
export declare function getNameAccountKey(hashed_name: Buffer, nameClass?: PublicKey, nameParent?: PublicKey): Promise<PublicKey>;
