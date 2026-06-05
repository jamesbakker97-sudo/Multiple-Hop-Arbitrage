# Production deployment (control-plane)

1) Prepare `.env` in the repository root with production values. DO NOT commit this file.

Required settings (examples):
- `ARBITRUM_RPC_URL` — your Arbitrum mainnet RPC endpoint
- `EXECUTOR_CONTRACT_ADDRESS` — deployed executor address
- `EXECUTOR_PRIVATE_KEY_PATH` — absolute path to key on the host (e.g. `C:\secrets\executor.key`)
- `EXECUTOR_ALLOW_INLINE_PRIVATE_KEY=false`

2) Place your executor key on the host and make it readable only by the runtime user.

3) Build & run (on the host):
```bash
./deploy/deploy.sh
```

4) Alternative: systemd
- Copy `deploy/control-plane.service` to `/etc/systemd/system/control-plane.service` and update `WorkingDirectory` and Exec paths.
- Reload and enable:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now control-plane.service
```

Notes & security:
- The compose file mounts `${EXECUTOR_PRIVATE_KEY_PATH}` into the container at `/run/secrets/executor.key` and sets `EXECUTOR_PRIVATE_KEY_PATH` accordingly. Ensure the path is absolute and accessible.
- Use a secrets manager or remote signer for improved security.
- Set `EXECUTOR_PAPER_TRADING=false` for live runs and increase confirmations if desired.
