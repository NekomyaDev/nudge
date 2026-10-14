/**
 * Nudge WASM Host Tool Bridge (Reference Implementation)
 *
 * Implements the memory-marshalling contract for `nudge_host_tool_call`.
 * Works in both browser environments and Node.js.
 */

// Host error codes returned to WebAssembly
export const HOST_ERROR = {
  SERVER_NOT_FOUND: -1,
  TOOL_NOT_FOUND: -2,
  EXECUTION_FAILED: -3,
  BUFFER_TOO_SMALL: -4,
  INVALID_ARGUMENTS: -5,
};

const textDecoder = new TextDecoder("utf-8");
const textEncoder = new TextEncoder();

/**
 * Default sample dispatcher supporting local tools and remote MCP forwarding.
 *
 * Note: Since `nudge_host_tool_call` is a synchronous C function, local tools
 * return the result string synchronously. For remote async MCP servers, hosts
 * can use WebAssembly JSPI, Asyncify, XMLHttpRequest, or worker messaging.
 *
 * @param {string} server Server identifier
 * @param {string} tool Tool name
 * @param {Record<string, unknown>} args Parsed arguments object
 * @returns {string} JSON string or plain text result
 */
export function defaultToolDispatcher(server, tool, args) {
  // Local mock tools
  if (server === "weather") {
    if (tool === "get_current") {
      const city = args.city || "Unknown";
      return JSON.stringify({
        city,
        temperature_c: 21,
        condition: "Partly Cloudy",
      });
    }
    throw new Error(`Unknown weather tool: ${tool}`);
  }

  if (server === "calculator") {
    if (tool === "add") {
      const a = Number(args.a ?? 0);
      const b = Number(args.b ?? 0);
      return JSON.stringify({ result: a + b });
    }
    throw new Error(`Unknown calculator tool: ${tool}`);
  }

  // Example: Remote HTTP MCP server endpoint via synchronous XMLHttpRequest in browser
  if (server.startsWith("http://") || server.startsWith("https://")) {
    if (typeof XMLHttpRequest !== "undefined") {
      const xhr = new XMLHttpRequest();
      xhr.open("POST", `${server}/tools/call`, false); // synchronous
      xhr.setRequestHeader("Content-Type", "application/json");
      xhr.send(
        JSON.stringify({
          method: "tools/call",
          params: { name: tool, arguments: args },
        })
      );
      if (xhr.status >= 200 && xhr.status < 300) {
        return xhr.responseText;
      }
      throw new Error(`Remote MCP returned HTTP ${xhr.status}`);
    }
    throw new Error("Synchronous remote MCP requires XMLHttpRequest or worker bridge");
  }

  // Server not recognized
  const err = new Error(`Server not found: ${server}`);
  err.code = HOST_ERROR.SERVER_NOT_FOUND;
  throw err;
}

/**
 * Creates the imports object containing `nudge_host_tool_call` for WebAssembly.instantiate.
 *
 * @param {() => WebAssembly.Memory} getMemory Accessor for the WASM instance linear memory
 * @param {(server: string, tool: string, args: any) => Promise<string> | string} [dispatcher]
 * @returns {{ env: { nudge_host_tool_call: Function } }}
 */
export function createHostToolImports(getMemory, dispatcher = defaultToolDispatcher) {
  return {
    env: {
      /**
       * Imported C function called by Nudge runtime in WASM.
       *
       * @param {number} serverPtr Pointer to server name UTF-8 bytes
       * @param {number} serverLen Length of server name UTF-8 bytes
       * @param {number} toolPtr Pointer to tool name UTF-8 bytes
       * @param {number} toolLen Length of tool name UTF-8 bytes
       * @param {number} argsPtr Pointer to arguments JSON UTF-8 bytes
       * @param {number} argsLen Length of arguments JSON UTF-8 bytes
       * @param {number} outPtr Pointer to allocated output buffer in WASM memory
       * @param {number} outCap Capacity in bytes of the output buffer
       * @returns {number} Non-negative bytes written, or negative error code
       */
      nudge_host_tool_call(
        serverPtr,
        serverLen,
        toolPtr,
        toolLen,
        argsPtr,
        argsLen,
        outPtr,
        outCap
      ) {
        try {
          const memory = getMemory();
          if (!memory || !memory.buffer) {
            console.error("[nudge-host] WASM memory not accessible");
            return HOST_ERROR.EXECUTION_FAILED;
          }

          // 1. Read input strings from WASM linear memory
          const serverBytes = new Uint8Array(memory.buffer, serverPtr, serverLen);
          const toolBytes = new Uint8Array(memory.buffer, toolPtr, toolLen);
          const argsBytes = new Uint8Array(memory.buffer, argsPtr, argsLen);

          const server = textDecoder.decode(serverBytes);
          const tool = textDecoder.decode(toolBytes);
          const argsJson = textDecoder.decode(argsBytes);

          let parsedArgs = {};
          try {
            parsedArgs = argsJson ? JSON.parse(argsJson) : {};
          } catch (e) {
            console.error("[nudge-host] Invalid JSON arguments:", e);
            return HOST_ERROR.INVALID_ARGUMENTS;
          }

          // 2. Dispatch tool execution synchronously or handle sync result
          // Note: synchronous C imports in WASM expect an immediate return code.
          let rawResult;
          try {
            rawResult = dispatcher(server, tool, parsedArgs);
          } catch (e) {
            console.error(`[nudge-host] Tool execution failed (${server}::${tool}):`, e);
            if (e && typeof e.code === "number") {
              return e.code;
            }
            if (e && e.message && e.message.includes("not found")) {
              return HOST_ERROR.TOOL_NOT_FOUND;
            }
            return HOST_ERROR.EXECUTION_FAILED;
          }

          const resultStr = typeof rawResult === "string" ? rawResult : JSON.stringify(rawResult);
          const resultBytes = textEncoder.encode(resultStr);

          // 3. Verify buffer capacity
          if (resultBytes.length > outCap) {
            console.warn(
              `[nudge-host] Output buffer too small: required ${resultBytes.length}, capacity ${outCap}`
            );
            return HOST_ERROR.BUFFER_TOO_SMALL;
          }

          // 4. Copy result into WASM memory output buffer
          const outBuffer = new Uint8Array(memory.buffer, outPtr, outCap);
          outBuffer.set(resultBytes);

          // Return number of bytes written
          return resultBytes.length;
        } catch (err) {
          console.error("[nudge-host] Unexpected exception in bridge:", err);
          return HOST_ERROR.EXECUTION_FAILED;
        }
      },
    },
  };
}
