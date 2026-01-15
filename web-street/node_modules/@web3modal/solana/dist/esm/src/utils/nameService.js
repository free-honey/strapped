import { PublicKey } from '@solana/web3.js';
export class NameRegistry {
    constructor(obj) {
        this.parentName = new PublicKey(obj.parentName);
        this.owner = new PublicKey(obj.owner);
        this.class = new PublicKey(obj.class);
    }
}
NameRegistry.schema = new Map([
    [
        NameRegistry,
        {
            kind: 'struct',
            fields: [
                ['parentName', [32]],
                ['owner', [32]],
                ['class', [32]]
            ]
        }
    ]
]);
//# sourceMappingURL=nameService.js.map