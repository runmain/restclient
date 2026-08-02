/**
 * @httpyac-type Hmac
 * Fluent hasher from crypto.createHmac — chain .update().digest()
 */
'use strict';

module.exports = {
  /**
   * @returns {Hmac}
   */
  update(data, inputEncoding) {},
  /**
   * @returns {string}
   */
  digest(encoding) {},
  /**
   * @returns {Hmac}
   */
  copy() {},
};
