/**
 * Pre-request signing helpers — edit THIS file for full vtsls / Node IntelliSense.
 * The .http file only `require()`s these exports (no heavy JS inside {{ }}).
 *
 * @typedef {object} HttpRequestLike
 * @property {string} [method]
 * @property {string} [url]
 * @property {Record<string, string>} [headers]
 * @property {unknown} [body]
 */

'use strict';

const crypto = require('crypto');

/**
 * Build HMAC-SHA256 signature headers for a request (httpyac pre-request style).
 *
 * @param {HttpRequestLike} request - httpyac `request` object from the script context
 * @param {string} [secret='secret']
 * @returns {{ authDate: string, authentication: string }}
 *
 * @example
 * // in .http:
 * // {{
 * //   const { signRequest } = require('./scripts/auth-sign.js');
 * //   const s = signRequest(request, 'secret');
 * //   exports.authDate = s.authDate;
 * //   exports.authentication = s.authentication;
 * // }}
 */
function signRequest(request, secret = 'secret') {
  const date = new Date();
  // Full Node crypto API works here under vtsls (crypto.createHmac → .update → .digest)
  const signatureBase64 = crypto
    .createHmac('sha256', secret)
    .update(`${request.method}\u2028${request.url}\u2028${date.getTime()}`)
    .digest('base64');

  return {
    authDate: date.toUTCString(),
    authentication: `Basic ${signatureBase64}`,
  };
}

/**
 * @param {string} data
 * @param {string} [algo='sha256']
 * @returns {string} hex digest
 */
function hashHex(data, algo = 'sha256') {
  return crypto.createHash(algo).update(data, 'utf8').digest('hex');
}

module.exports = {
  signRequest,
  hashHex,
};
