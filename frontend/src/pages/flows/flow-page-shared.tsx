export { formatError } from "@/lib/utils";

export function CenteredMessage({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground">
      <p className="text-sm">{title}</p>
      <p className="max-w-md text-xs font-mono">{detail}</p>
    </div>
  );
}
