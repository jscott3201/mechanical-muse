import { CircleAlert } from "lucide-react";

import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

/**
 * A run the URL names but the server does not list. Runs live in the server's memory, so a
 * restart clears them; a link or a refresh from before it lands here instead of waiting forever
 * on a stream that will never start (and offering a Stop that can only fail).
 */
export function RunMissing({ runId }: { runId: string }) {
  return (
    <Card role="status">
      <CardHeader>
        <CardTitle className="flex flex-wrap items-center gap-2">
          <CircleAlert aria-hidden="true" className="size-4 text-destructive" />
          <span>Run not found</span>
          <span className="font-mono text-xs font-normal text-muted-foreground">{runId}</span>
        </CardTitle>
        <CardDescription>
          This run is not on this server (runs are kept in memory, so a restart clears them). Start a new run, or
          replay a recorded one from the replay list beside the run controls.
        </CardDescription>
      </CardHeader>
    </Card>
  );
}
