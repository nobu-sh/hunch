// DevTools helpers for what page scripts can't reach.

/**
 * Runs `functionDeclaration` with `this` bound to the shadow root of the first `hostName` element,
 * even a closed one, and returns its result by value. Returns null when no such host is mounted.
 */
export async function callInShadowRoot({ page, hostName, functionDeclaration }) {
  const session = await page.createCDPSession();
  try {
    const { root } = await session.send('DOM.getDocument', { depth: -1, pierce: true });
    const shadowRoot = findShadowRoot({ node: root, hostName });
    if (!shadowRoot) return null;
    const { object } = await session.send('DOM.resolveNode', {
      backendNodeId: shadowRoot.backendNodeId,
    });
    const { result } = await session.send('Runtime.callFunctionOn', {
      objectId: object.objectId,
      returnByValue: true,
      functionDeclaration,
    });
    return result.value;
  } finally {
    await session.detach();
  }
}

function findShadowRoot({ node, hostName }) {
  if (node.localName === hostName) return node.shadowRoots?.[0] ?? null;
  for (const child of [...(node.children ?? []), ...(node.shadowRoots ?? [])]) {
    const found = findShadowRoot({ node: child, hostName });
    if (found) return found;
  }
  return null;
}
