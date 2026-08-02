/**
 * Builtin httpyac `request` (pre-request, mutable).
 * User `.script/request.js` overrides same member names.
 */
'use strict';

module.exports = {
  url: null, // Request URL (mutable in pre-request)
  method: null, // HTTP method
  headers: {
    // Request headers — extend with .script/request.headers.js
  },
  body: null, // Request body
  contentType: null, // Parsed content-type
  protocol: null, // Protocol
  timeout: null, // Timeout (ms)
  proxy: null, // Proxy URL
  noRedirect: null, // Disable following redirects
  noRejectUnauthorized: null, // Skip TLS verify
  supportsStreaming: null, // Streaming support flag
  options: null, // Underlying got options (advanced)
};
