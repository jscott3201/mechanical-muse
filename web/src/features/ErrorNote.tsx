import { CircleAlert } from "lucide-react";

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import type { ApiFailure } from "@/lib/api";

/** An error with its stable code and the server's message, shown as text. */
export function ErrorNote({
  error,
  onRetry,
  hint,
}: {
  error: ApiFailure;
  onRetry?: (() => void) | undefined;
  /** What the viewer can do instead, when trying again cannot help. */
  hint?: string | undefined;
}) {
  return (
    <Alert variant="destructive">
      <CircleAlert aria-hidden="true" />
      <AlertTitle>{error.code}</AlertTitle>
      <AlertDescription>
        <p>{error.message}</p>
        {hint && <p>{hint}</p>}
        {onRetry && (
          <Button variant="outline" size="sm" onClick={onRetry}>
            Try again
          </Button>
        )}
      </AlertDescription>
    </Alert>
  );
}

/** The code a model-backed route answers (503) on a server with no model key. */
const NO_MODEL = "no-model";

const NO_MODEL_HINT =
  "This server has no model key, so it can show only explanations recorded earlier. A new explanation needs a model key on the server; trying again will not help.";

/**
 * An explanation request that failed. On a keyless server (`no-model`) a retry can never succeed,
 * so it is not offered; the note says what is missing instead.
 */
export function ExplainErrorNote({ error, onRetry }: { error: ApiFailure; onRetry: () => void }) {
  const keyless = error.code === NO_MODEL;
  return <ErrorNote error={error} onRetry={keyless ? undefined : onRetry} hint={keyless ? NO_MODEL_HINT : undefined} />;
}
