export interface LitRuntime {
  Actions: {
    getLitActionPrivateKey(): Promise<string>;
    getLitActionPublicKey(params: { ipfsId: string }): Promise<string>;
  };
}
