import { CircleCheck, CircleX } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { formatNumber, formatWire } from "@/lib/format";
import type { Receipt as ReceiptDto } from "@/types/generated/Receipt";
import type { ScenarioReport } from "@/types/generated/ScenarioReport";

function Outcome({ s }: { s: ScenarioReport }) {
  switch (s.result) {
    case "pass":
      return <span className="text-muted-foreground">all expectations held</span>;
    case "fail":
      return (
        <span>
          {s.output} at {formatNumber(s.t, 1)} s: expected {formatWire(s.expected)}, got{" "}
          {formatWire(s.actual)} (window {formatNumber(s.from_s, 1)}–{formatNumber(s.to_s, 1)} s)
        </span>
      );
    case "error":
      return (
        <span>
          {s.code}: {s.detail}
          {s.t !== undefined && ` at ${formatNumber(s.t, 1)} s`}
        </span>
      );
  }
}

/** A verification receipt: content id, pass count, and every scenario's result. */
export function Receipt({ receipt }: { receipt: ReceiptDto }) {
  const passed = receipt.scenarios.filter((s) => s.result === "pass").length;
  return (
    <section aria-label="Verification receipt" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        {receipt.passed ? (
          <Badge>
            <CircleCheck aria-hidden="true" /> Verified
          </Badge>
        ) : (
          <Badge variant="destructive">
            <CircleX aria-hidden="true" /> Not verified
          </Badge>
        )}
        <span className="text-sm">
          passes {passed} of {receipt.scenarios.length} scenarios
        </span>
        <code className="text-xs text-muted-foreground break-all">{receipt.content_id}</code>
      </div>
      <div className="overflow-x-auto">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Scenario</TableHead>
              <TableHead>Result</TableHead>
              <TableHead className="text-right">Frames</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {receipt.scenarios.map((s) => (
              <TableRow key={s.name}>
                <TableCell className="font-mono text-xs">{s.name}</TableCell>
                <TableCell className="whitespace-normal">
                  <span className="inline-flex items-start gap-1">
                    {s.result === "pass" ? (
                      <CircleCheck aria-label="pass" className="size-4 shrink-0" />
                    ) : (
                      <CircleX aria-label={s.result} className="size-4 shrink-0 text-destructive" />
                    )}
                    <Outcome s={s} />
                  </span>
                </TableCell>
                <TableCell className="text-right tabular-nums">{s.ticks}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </section>
  );
}
