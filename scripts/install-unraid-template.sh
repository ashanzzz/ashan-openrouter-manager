#!/bin/bash
set -euo pipefail

APPDIR="${APPDIR:-/mnt/cache/appdata/ashan-openrouter-manager}"
TEMPLATE_DIR="${UNRAID_TEMPLATE_DIR:-/boot/config/plugins/dockerMan/templates-user}"
TEMPLATE="${TEMPLATE_DIR}/my-ashan-openrouter-manager.xml"
KEYFILE="${APPDIR}/.master_key"
HOST_PORT="${HOST_PORT:-8080}"

mkdir -p "$APPDIR" "$TEMPLATE_DIR"

if [ -s "$KEYFILE" ]; then
  APP_MASTER_KEY="$(cat "$KEYFILE")"
else
  APP_MASTER_KEY="$(openssl rand -hex 32)"
  printf '%s' "$APP_MASTER_KEY" > "$KEYFILE"
  chmod 600 "$KEYFILE"
fi

cat > "$TEMPLATE" <<EOF
<?xml version="1.0"?>
<Container version="2">
  <Name>ashan-openrouter-manager</Name>
  <Repository>ghcr.io/ashanzzz/ashan-openrouter-manager:latest</Repository>
  <Registry>https://github.com/ashanzzz/ashan-openrouter-manager/pkgs/container/ashan-openrouter-manager</Registry>
  <Network>bridge</Network>
  <MyIP/>
  <Shell>sh</Shell>
  <Privileged>false</Privileged>
  <Support>https://github.com/ashanzzz/ashan-openrouter-manager/issues</Support>
  <Project>https://github.com/ashanzzz/ashan-openrouter-manager</Project>
  <Overview>OpenRouter free-model selector and New API synchronizer. One container serves the React UI, Rust API, scheduler and SQLite state.</Overview>
  <Category>Tools:</Category>
  <WebUI>http://[IP]:[PORT:8080]/</WebUI>
  <TemplateURL/>
  <Icon/>
  <ExtraParams/>
  <PostArgs/>
  <CPUset/>
  <DateInstalled>$(date +%s)</DateInstalled>
  <DonateText/>
  <DonateLink/>
  <Requires/>

  <Config Name="WebUI Port"
          Target="8080"
          Default="$HOST_PORT"
          Mode="tcp"
          Description="Host port for the Web UI and API. The container always listens on internal port 8080."
          Type="Port"
          Display="always"
          Required="true"
          Mask="false">$HOST_PORT</Config>

  <Config Name="App Data"
          Target="/data"
          Default="$APPDIR"
          Mode="rw"
          Description="SQLite database and persistent application data."
          Type="Path"
          Display="always"
          Required="true"
          Mask="false">$APPDIR</Config>

  <Config Name="APP_MASTER_KEY"
          Target="APP_MASTER_KEY"
          Default="$APP_MASTER_KEY"
          Mode=""
          Description="Encryption root key. Keep this value stable after saving secrets."
          Type="Variable"
          Display="advanced-hide"
          Required="true"
          Mask="true">$APP_MASTER_KEY</Config>

  <Config Name="RUST_LOG"
          Target="RUST_LOG"
          Default="info"
          Mode=""
          Description="Rust tracing filter."
          Type="Variable"
          Display="advanced"
          Required="false"
          Mask="false">info</Config>
</Container>
EOF

printf '\nTemplate installed: %s\n' "$TEMPLATE"
printf 'Persistent data:   %s\n' "$APPDIR"
printf 'Host WebUI port:   %s\n' "$HOST_PORT"
printf 'Internal port:     8080 (fixed, not user-configurable)\n'
printf 'Master key file:   %s\n\n' "$KEYFILE"
printf 'Open Unraid -> Docker -> Add Container -> Template -> ashan-openrouter-manager\n'
