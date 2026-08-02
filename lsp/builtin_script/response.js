/**
 * Builtin httpyac `response` (post-request / @forceRef).
 */
'use strict';

module.exports = {
  statusCode: null, // HTTP status code (number)
  status: null, // Status code (alias / JetBrains-style)
  statusMessage: null, // Reason phrase
  headers: {
    valueOf(name) {}, // headers.valueOf("Name") — JetBrains-style
    get(name) {}, // headers.get("Name") / bracket access
  },
  body: null, // Body (string / object depending on content)
  parsedBody: null, // Parsed JSON/XML body when available
  prettyPrintBody: null, // Pretty-printed body string
  contentType: null, // Parsed content-type
  rawBody: null, // Raw body Buffer
  rawHeaders: null, // Raw header lines
  httpVersion: null, // HTTP version string
  protocol: null, // Protocol (HTTP, etc.)
  name: null, // Response / request name if set
  request: null, // The request object that produced this response
  timings: null, // Timing phases (HttpTimings)
  meta: null, // Extra metadata map
  tags: null, // Tags array
  responseTime: null, // Timing (ms), if available (alias)
};
