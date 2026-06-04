# Example Remote Signer

This is a minimal example signer for development and testing only. Do NOT use this in production — it stores raw private keys in memory and exposes an unauthenticated signing endpoint.

Files:
- `index.js` — small Express app exposing `/address` and `/sign`.

Run locally:

1. Copy the example env file to `.env` and set a private key:

```
cp ../../.env.example .env
# edit .env and set SIGNER_PRIVATE_KEY
```

2. Install and start:

```
cd examples/remote-signer
npm install
npm start
```

3. Test manually:

```
curl http://localhost:3001/address
curl -X POST http://localhost:3001/sign -H "Content-Type: application/json" -d '{"tx": {"to":"0x0000000000000000000000000000000000000000","value":"0x0"}}'
```

Automated test:

```
cd examples/remote-signer
npm test
```
