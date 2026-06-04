import type { TransactionRequest } from "ethers";

export class RemoteSigner {
  private url: string;

  constructor(url: string) {
    this.url = url.replace(/\/$/, "");
  }

  async getAddress(): Promise<string> {
    const res = await fetch(`${this.url}/address`, {
      method: "GET",
      headers: { "content-type": "application/json" },
    });
    if (!res.ok) {
      throw new Error(`remote signer address fetch failed: ${res.status} ${res.statusText}`);
    }
    const body = await res.json();
    if (!body || !body.address) {
      throw new Error("remote signer address response missing 'address'");
    }
    return String(body.address).toLowerCase();
  }

  async sign(tx: TransactionRequest): Promise<string> {
    const serializable: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(tx)) {
      if (v === undefined) continue;
      if (typeof v === "bigint") {
        serializable[k] = v.toString();
      } else if (typeof v === "object" && v !== null && (v as any)._isBigNumber) {
        serializable[k] = String(v);
      } else {
        serializable[k] = v;
      }
    }

    const res = await fetch(`${this.url}/sign`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tx: serializable }),
    });
    if (!res.ok) {
      const txt = await res.text();
      throw new Error(`remote signer sign failed: ${res.status} ${res.statusText} - ${txt}`);
    }
    const body = await res.json();
    if (!body || !body.signedTx) {
      throw new Error("remote signer response missing 'signedTx'");
    }
    return String(body.signedTx);
  }
}

export default RemoteSigner;
