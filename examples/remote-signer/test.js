import { spawn } from "child_process";
import fetch from "node-fetch";

async function waitForServer(proc) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("server did not start in time")), 5000);
    proc.stdout.on("data", (d) => {
      const s = d.toString();
      if (s.includes("Remote signer listening")) {
        clearTimeout(timeout);
        resolve(true);
      }
    });
    proc.on("exit", (code) => reject(new Error(`server exited (${code})`)));
  });
}

async function main() {
  const proc = spawn(process.execPath, ["./index.js"], {
    env: { ...process.env, SIGNER_PRIVATE_KEY: process.env.SIGNER_PRIVATE_KEY || "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" },
    cwd: process.cwd(),
    stdio: ["ignore", "pipe", "inherit"],
  });

  try {
    await waitForServer(proc);
    const res = await fetch("http://localhost:3001/address");
    const body = await res.json();
    console.log("address", body.address);

    const signRes = await fetch("http://localhost:3001/sign", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tx: { to: "0x0000000000000000000000000000000000000000", value: "0x0" } }),
    });
    const signBody = await signRes.json();
    console.log("signedTx length", (signBody.signedTx || "").length);
    proc.kill();
    process.exit(0);
  } catch (err) {
    console.error(err);
    proc.kill();
    process.exit(1);
  }
}

main();
