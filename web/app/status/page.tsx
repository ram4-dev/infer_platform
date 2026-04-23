import { Suspense } from "react";
import Link from "next/link";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "@/components/ui/separator";

// ---------- Types ----------

type NodeStatus = "online" | "offline" | "busy" | "degraded";

interface NodeInfo {
  id: string;
  name: string;
  host: string;
  port: number;
  gpu_name: string;
  vram_mb: number;
  status: NodeStatus;
  model?: string;
  license?: string;
  registered_at: string;
  last_seen: string;
}

// ---------- Data fetchers ----------

async function fetchNodes(): Promise<NodeInfo[]> {
  const base =
    process.env.NEXT_PUBLIC_SITE_URL ??
    process.env.VERCEL_URL ??
    "http://localhost:3000";
  const url = `${base.startsWith("http") ? base : `https://${base}`}/api/nodes`;

  try {
    const res = await fetch(url, { cache: "no-store" });
    if (!res.ok) return [];
    const body = await res.json();
    return body.data ?? [];
  } catch {
    return [];
  }
}

function modelsFromNodes(nodes: NodeInfo[]): Array<{ id: string; count: number }> {
  const counts = new Map<string, number>();
  for (const node of nodes) {
    if (node.status !== "online") continue;
    if (!node.model) continue;
    counts.set(node.model, (counts.get(node.model) ?? 0) + 1);
  }

  return [...counts.entries()]
    .map(([id, count]) => ({ id, count }))
    .sort((a, b) => a.id.localeCompare(b.id));
}

// ---------- Page ----------

export default function StatusPage() {
  return (
    <main className="flex flex-col min-h-screen">
      {/* Nav */}
      <nav className="border-b border-border px-6 py-4 flex items-center justify-between">
        <Link
          href="/"
          className="font-mono text-sm font-semibold tracking-tight hover:text-muted-foreground transition-colors"
        >
          infer.ram4.dev
        </Link>
        <div className="flex items-center gap-4 text-sm text-muted-foreground">
          <Link
            href="/status"
            className="text-foreground font-medium text-xs font-mono"
          >
            Status
          </Link>
          <Link
            href="/keys"
            className="hover:text-foreground transition-colors text-xs"
          >
            API Keys
          </Link>
          <Link
            href="/provider"
            className="hover:text-foreground transition-colors text-xs"
          >
            Provider
          </Link>
        </div>
      </nav>

      <div className="flex-1 px-6 py-10 max-w-5xl mx-auto w-full space-y-10">
        {/* Summary cards */}
        <Suspense fallback={<SummaryCardsSkeleton />}>
          <SummaryCards />
        </Suspense>

        <Separator />

        {/* Nodes table */}
        <section>
          <h2 className="text-base font-semibold mb-4">Connected nodes</h2>
          <Suspense fallback={<TableSkeleton />}>
            <NodesTable />
          </Suspense>
        </section>

        <Separator />

        {/* Models list */}
        <section>
          <h2 className="text-base font-semibold mb-4">Available models</h2>
          <Suspense fallback={<ModelsSkeleton />}>
            <ModelsList />
          </Suspense>
        </section>
      </div>
    </main>
  );
}

// ---------- Dynamic components ----------

async function SummaryCards() {
  const nodes = await fetchNodes();

  const onlineNodes = nodes.filter((n) => n.status === "online");
  const totalVram = nodes.reduce((sum, n) => sum + n.vram_mb, 0);
  const totalVramGb = (totalVram / 1024).toFixed(1);
  const models = modelsFromNodes(nodes);

  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
      <StatCard
        title="Nodes online"
        value={`${onlineNodes.length} / ${nodes.length}`}
        sub={nodes.length === 0 ? "No nodes registered" : "GPU providers"}
      />
      <StatCard
        title="Total VRAM"
        value={`${totalVramGb} GB`}
        sub="Across all nodes"
      />
      <StatCard
        title="Models available"
        value={String(models.length)}
        sub={models.length === 0 ? "No online model capacity" : "Online model pools"}
      />
    </div>
  );
}

async function NodesTable() {
  const nodes = await fetchNodes();

  if (nodes.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No nodes registered. Start a node agent to join the network.
      </p>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
          <TableHead>GPU</TableHead>
          <TableHead>VRAM</TableHead>
          <TableHead>Host</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Model</TableHead>
          <TableHead>Last seen</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {nodes.map((node) => (
          <TableRow key={node.id}>
            <TableCell className="font-mono text-xs">{node.name}</TableCell>
            <TableCell className="text-xs">{node.gpu_name}</TableCell>
            <TableCell className="font-mono text-xs">
              {(node.vram_mb / 1024).toFixed(1)} GB
            </TableCell>
            <TableCell className="font-mono text-xs">
              {node.host}:{node.port}
            </TableCell>
            <TableCell>
              <NodeStatusBadge status={node.status} />
            </TableCell>
            <TableCell className="font-mono text-xs">
              {node.model ?? "—"}
            </TableCell>
            <TableCell className="text-xs text-muted-foreground">
              {new Date(node.last_seen).toLocaleTimeString()}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

async function ModelsList() {
  const nodes = await fetchNodes();
  const models = modelsFromNodes(nodes);

  if (models.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">
        No models available. Register online nodes with a configured model.
      </p>
    );
  }

  return (
    <div className="flex flex-wrap gap-2">
      {models.map((model) => (
        <Badge key={model.id} variant="secondary" className="font-mono text-xs">
          {model.id} · {model.count} node{model.count === 1 ? "" : "s"}
        </Badge>
      ))}
    </div>
  );
}

// ---------- Helper components ----------

function StatCard({
  title,
  value,
  sub,
}: {
  title: string;
  value: string;
  sub: string;
}) {
  return (
    <Card>
      <CardHeader className="pb-1">
        <CardTitle className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-3xl font-bold font-mono">{value}</p>
        <p className="text-xs text-muted-foreground mt-1">{sub}</p>
      </CardContent>
    </Card>
  );
}

function NodeStatusBadge({ status }: { status: NodeStatus }) {
  const variants: Record<NodeStatus, "default" | "secondary" | "destructive"> =
    {
      online: "default",
      busy: "secondary",
      degraded: "secondary",
      offline: "destructive",
    };
  return (
    <Badge variant={variants[status]} className="text-xs font-mono">
      {status}
    </Badge>
  );
}

// ---------- Skeleton fallbacks ----------

function SummaryCardsSkeleton() {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
      {[0, 1, 2].map((i) => (
        <Card key={i}>
          <CardHeader className="pb-1">
            <Skeleton className="h-3 w-24" />
          </CardHeader>
          <CardContent>
            <Skeleton className="h-8 w-16 mb-2" />
            <Skeleton className="h-3 w-32" />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function TableSkeleton() {
  return (
    <div className="space-y-2">
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} className="h-10 w-full" />
      ))}
    </div>
  );
}

function ModelsSkeleton() {
  return (
    <div className="flex gap-2">
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} className="h-6 w-24 rounded-full" />
      ))}
    </div>
  );
}
