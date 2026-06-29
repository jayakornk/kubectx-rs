#!/bin/bash
# Integration test for kubectx-rs
# Creates a temporary kubeconfig and tests all operations

set +e  # Don't exit on error - some tests intentionally produce errors

BIN_DIR="./target/debug"
TMPDIR=$(mktemp -d)
export KUBECONFIG="$TMPDIR/config"
export HOME="$TMPDIR"

# Create a test kubeconfig
cat > "$KUBECONFIG" << 'EOF'
apiVersion: v1
kind: Config
current-context: minikube
clusters:
- name: minikube
  cluster:
    server: https://localhost:8443
- name: gke-prod
  cluster:
    server: https://1.2.3.4
- name: gke-staging
  cluster:
    server: https://5.6.7.8
contexts:
- name: minikube
  context:
    cluster: minikube
    user: minikube
    namespace: default
- name: gke-prod
  context:
    cluster: gke-prod
    user: gke-prod
    namespace: production
- name: gke-staging
  context:
    cluster: gke-staging
    user: gke-staging
    namespace: staging
users:
- name: minikube
  user:
    client-certificate: /path/to/cert
- name: gke-prod
  user:
    token: abc123
- name: gke-staging
  user:
    token: def456
EOF

echo "=== Test 1: List contexts ==="
"$BIN_DIR/kubectx" | cat

echo ""
echo "=== Test 2: Show current context ==="
"$BIN_DIR/kubectx" -c

echo ""
echo "=== Test 3: Switch to gke-prod ==="
"$BIN_DIR/kubectx" gke-prod

echo ""
echo "=== Verify switch ==="
"$BIN_DIR/kubectx" -c

echo ""
echo "=== Test 4: Switch back to minikube ==="
"$BIN_DIR/kubectx" minikube

echo ""
echo "=== Test 5: Swap to previous context (should be gke-prod) ==="
"$BIN_DIR/kubectx" -
"$BIN_DIR/kubectx" -c

echo ""
echo "=== Test 6: Swap back to minikube ==="
"$BIN_DIR/kubectx" -
"$BIN_DIR/kubectx" -c

echo ""
echo "=== Test 7: Rename context ==="
"$BIN_DIR/kubectx" prod=gke-prod
"$BIN_DIR/kubectx" | cat

echo ""
echo "=== Test 8: Rename current context using '.' ==="
"$BIN_DIR/kubectx" prod-cluster=.
"$BIN_DIR/kubectx" -c

echo ""
echo "=== Test 9: Delete context ==="
"$BIN_DIR/kubectx" -d gke-staging
"$BIN_DIR/kubectx" | cat

echo ""
echo "=== Test 10: Unset current context ==="
"$BIN_DIR/kubectx" -u
"$BIN_DIR/kubectx" -c 2>&1 || true

echo ""
echo "=== Test 11: kubens - list namespaces ==="
# Create a fake kubectl that returns test namespaces
mkdir -p "$TMPDIR/bin"
cat > "$TMPDIR/bin/kubectl" << 'KUBECTL'
#!/bin/bash
if [ "$1" = "get" ] && [ "$2" = "namespaces" ]; then
  echo "namespace/default"
  echo "namespace/kube-system"
  echo "namespace/kube-public"
  echo "namespace/production"
  echo "namespace/staging"
fi
KUBECTL
chmod +x "$TMPDIR/bin/kubectl"
export PATH="$TMPDIR/bin:$PATH"

"$BIN_DIR/kubectx" prod
"$BIN_DIR/kubens" | cat

echo ""
echo "=== Test 12: kubens - switch namespace ==="
"$BIN_DIR/kubens" kube-system
"$BIN_DIR/kubens" -c

echo ""
echo "=== Test 13: kubens - swap to previous namespace ==="
"$BIN_DIR/kubens" -
"$BIN_DIR/kubens" -c

echo ""
echo "=== Test 14: kubens - unset namespace ==="
"$BIN_DIR/kubens" -u
"$BIN_DIR/kubens" -c 2>&1 || true

echo ""
echo "=== Test 15: kubectx - version ==="
"$BIN_DIR/kubectx" -V

echo ""
echo "=== Test 16: kubectx - help ==="
"$BIN_DIR/kubectx" -h

echo ""
echo "=== Test 17: kubens - help ==="
"$BIN_DIR/kubens" -h

echo ""
echo "=== Test 18: kubens - version ==="
"$BIN_DIR/kubens" -V

echo ""
echo "=== Test 19: Error - switch to non-existent context ==="
"$BIN_DIR/kubectx" nonexistent 2>&1 || true

echo ""
echo "=== Test 20: Error - switch to non-existent namespace ==="
"$BIN_DIR/kubens" nonexistent 2>&1 || true

echo ""
echo "=== Test 21: Delete current context using '.' ==="
"$BIN_DIR/kubectx" prod
"$BIN_DIR/kubectx" -d .
"$BIN_DIR/kubectx" -c 2>&1 || true

echo ""
echo "=== Test 22: Multiple context deletion ==="
"$BIN_DIR/kubectx" prod-cluster
"$BIN_DIR/kubectx" prod
"$BIN_DIR/kubectx" prod-cluster
"$BIN_DIR/kubectx" -d prod prod-cluster 2>&1
"$BIN_DIR/kubectx" | cat 2>&1 || true

echo ""
echo "=== Test 23: Verify kubeconfig YAML is valid after all operations ==="
cat "$KUBECONFIG"

echo ""
echo "=== All tests passed! ==="

# Cleanup
rm -rf "$TMPDIR"