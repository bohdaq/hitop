/**
 * ID Generator Utility
 * 
 * Generates unique IDs for requests, collections, tabs, and history items
 */

let lastTimestamp = 0;
let counter = 0;

/**
 * Generates a unique ID
 * Combines timestamp with a counter to ensure uniqueness even when called multiple times in the same millisecond
 * 
 * @returns {number} - Unique ID
 */
export const generateUniqueId = () => {
  const timestamp = Date.now();
  
  if (timestamp === lastTimestamp) {
    // Same millisecond, increment counter
    counter++;
  } else {
    // New millisecond, reset counter
    lastTimestamp = timestamp;
    counter = 0;
  }
  
  // Combine timestamp with counter to ensure uniqueness
  // Multiply timestamp by 1000 and add counter (supports up to 999 IDs per millisecond)
  return timestamp * 1000 + counter;
};

export default {
  generateUniqueId
};
