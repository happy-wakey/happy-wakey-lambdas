export const SCHEMA_VERSION = "ores.lambda-command.v1";
export const MAX_INVOCATION_BYTES = 256 * 1024;

const encoder = new TextEncoder();

function byteLength(value) {
  return encoder.encode(value).byteLength;
}

function requestId(event, context) {
  const headers = event && typeof event === "object" ? event.headers : undefined;
  return headers?.["x-request-id"] ||
    headers?.["X-Request-Id"] ||
    context?.awsRequestId ||
    event?.requestId ||
    "local";
}

function providerFor(event) {
  if (event?.provider) return event.provider;
  if (typeof process !== "undefined" && process.env?.AWS_LAMBDA_FUNCTION_NAME) return "aws-lambda";
  if (typeof process !== "undefined" && process.env?.K_SERVICE) return "gcp-cloud-run";
  if (typeof process !== "undefined" && process.env?.FUNCTIONS_WORKER_RUNTIME) return "azure-functions";
  return "local";
}

export function invoke(raw, provider = "local", fallbackRequestId = "local") {
  const text = typeof raw === "string" ? raw : JSON.stringify(raw ?? {});
  if (byteLength(text) > MAX_INVOCATION_BYTES) {
    return {
      requestId: fallbackRequestId,
      provider,
      ok: false,
      error: { code: "invocation_too_large", message: "invocation exceeds the size limit" }
    };
  }

  let command;
  try {
    command = JSON.parse(text);
  } catch {
    return {
      requestId: fallbackRequestId,
      provider,
      ok: false,
      error: { code: "invalid_invocation", message: "invocation is not valid JSON" }
    };
  }

  if (!command || typeof command !== "object" || Array.isArray(command)) {
    return {
      requestId: fallbackRequestId,
      provider,
      ok: false,
      error: { code: "invalid_invocation", message: "invocation must be an object" }
    };
  }

  return {
    requestId: command.requestId || fallbackRequestId,
    provider: command.provider || provider,
    ok: true,
    operation: command.command?.operation || "echo",
    result: command
  };
}

export async function handler(event, context = {}) {
  const raw = event?.body ?? event;
  const receipt = invoke(raw, providerFor(event), requestId(event, context));
  return {
    statusCode: receipt.ok ? 200 : 400,
    headers: { "content-type": "application/json" },
    body: JSON.stringify(receipt)
  };
}

export default handler;
