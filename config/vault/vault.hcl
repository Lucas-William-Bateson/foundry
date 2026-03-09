# =============================================================================
# Vault server configuration — production mode
#
# Runs on the same machine as foundry, listening on all interfaces within
# the Docker network. TLS is disabled since traffic stays inside the
# compose network (vault:8200).
# =============================================================================

storage "file" {
  path = "/vault/data"
}

listener "tcp" {
  address     = "0.0.0.0:8200"
  tls_disable = true
}

api_addr = "http://127.0.0.1:8200"

disable_mlock = true

ui = true
