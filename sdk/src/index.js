/**
 * Evore SDK
 * 
 * JavaScript SDK for the Evore automining program on Solana.
 * Built with @solana/web3.js
 * 
 * @example
 * const { 
 *   createManagerInstruction, 
 *   getDeployerPda,
 *   decodeDeployer 
 * } = require('evore-sdk');
 */

// Re-export everything from submodules
module.exports = {
  ...require('./constants'),
  ...require('./pda'),
  ...require('./accounts'),
  ...require('./instructions'),
  ...require('./transactions'),
};
