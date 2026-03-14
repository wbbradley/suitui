# suitui

## Dependencies

- `sui-rpc` and `sui-sdk-types` come from `sui-rust-sdk.git`, which releases independently from
  both `sui` and `walrus`. Always use the latest commit from that repo rather than pinning to
  walrus's rev. After initial buildout, manually bump as needed.

## UI/UX

- Visual elements should be greyed out when their data is stale (e.g., after network error or env
  switch). This is punted to vNext — don't over-invest in staleness handling in v1 beyond basic
  graceful error display.

## Understanding sui gRPC api

Read the .proto files in the MystenLabs/sui-rust-sdk repo under
crates/sui-rpc/vendored/proto/sui/rpc/v2 (typically in ../sui-rust-sdk).
