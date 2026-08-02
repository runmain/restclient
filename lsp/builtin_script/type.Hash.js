/**
 * @httpyac-type Hash
 * From crypto.createHash — chain .update().digest()
 */
'use strict';

module.exports = {
  /**
   * @returns {Hash}
   */
  update(data, inputEncoding) {},
  /**
   * @returns {string}
   */
  digest(encoding) {},
  /**
   * @returns {Hash}
   */
  copy() {},
};
