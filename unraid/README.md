# Unraid deployment

The container has one fixed internal HTTP port: **8080**. In Unraid, only the host-side **WebUI Port** is configurable. There is no separate frontend/backend port.

Run on the Unraid terminal after copying/cloning the project:

```bash
bash scripts/install-unraid-template.sh
```

Optional host port override:

```bash
HOST_PORT=18080 bash scripts/install-unraid-template.sh
```

Persistent data defaults to:

```text
/mnt/cache/appdata/ashan-openrouter-manager
```

The installer generates `APP_MASTER_KEY` once and stores it in `.master_key` under the appdata directory so recreating the container does not silently change the encryption key.
