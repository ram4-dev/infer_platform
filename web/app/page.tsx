import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";

export default function Home() {
  return (
    <main className="flex flex-col min-h-screen">
      {/* Nav */}
      <nav className="border-b border-border px-6 py-4 flex items-center justify-between">
        <span className="font-mono text-sm font-semibold tracking-tight">
          infer.ram4.dev
        </span>
        <div className="flex items-center gap-4 text-sm text-muted-foreground">
          <Link
            href="/status"
            className="hover:text-foreground transition-colors"
          >
            Status
          </Link>
          <Link
            href="/keys"
            className="hover:text-foreground transition-colors"
          >
            API Keys
          </Link>
          <a
            href="https://github.com/hyperspaceai/agi/blob/main/docs/PODS.md"
            target="_blank"
            rel="noopener noreferrer"
            className="hover:text-foreground transition-colors"
          >
            Docs
          </a>
        </div>
      </nav>

      {/* Hero */}
      <section className="flex-1 flex flex-col items-center justify-center px-6 py-24 text-center">
        <Badge variant="secondary" className="mb-6 font-mono text-xs">
          OpenAI-compatible · Distributed · Open
        </Badge>
        <h1 className="text-5xl font-bold tracking-tight mb-4 max-w-2xl leading-tight">
          Distributed AI inference at your fingertips
        </h1>
        <p className="text-muted-foreground text-lg max-w-xl mb-10">
          Pool consumer GPU devices into model-specific clusters and route each
          request to a healthy node with predictable failover.
        </p>
        <div className="flex items-center gap-4">
          <Link
            href="/status"
            className="inline-flex h-10 items-center rounded-md bg-primary px-6 text-sm font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
          >
            View network status
          </Link>
          <a
            href="#quickstart"
            className="inline-flex h-10 items-center rounded-md border border-border px-6 text-sm font-medium hover:bg-accent transition-colors"
          >
            Quick start
          </a>
        </div>
      </section>

      <Separator />

      {/* Features */}
      <section className="px-6 py-16 max-w-5xl mx-auto w-full">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
          <Feature
            icon="⚡"
            title="OpenAI-compatible"
            description="Drop-in replacement for the OpenAI API. Works with any SDK that speaks /v1/chat/completions."
          />
          <Feature
            icon="🌐"
            title="Distributed GPU pool"
            description="Nodes anywhere on the internet contribute VRAM. Requests are balanced across nodes serving the same model."
          />
          <Feature
            icon="🔑"
            title="Bearer token access"
            description="Simple pk_* API keys control access. Rate limiting and usage tracking built in."
          />
        </div>
      </section>

      <Separator />

      {/* Quick start */}
      <section id="quickstart" className="px-6 py-16 max-w-3xl mx-auto w-full">
        <h2 className="text-xl font-semibold mb-6">Quick start</h2>
        <pre className="bg-card border border-border rounded-lg p-5 text-sm font-mono overflow-x-auto text-muted-foreground">
          <code>{`curl https://infer.ram4.dev/v1/chat/completions \\
  -H "Authorization: Bearer pk_your_key" \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": "llama3.2",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'`}</code>
        </pre>
      </section>

      {/* Footer */}
      <footer className="border-t border-border px-6 py-6 text-center text-xs text-muted-foreground">
        infer.ram4.dev · powered by Go + Chi · inspired by{" "}
        <a
          href="https://github.com/hyperspaceai/agi/blob/main/docs/PODS.md"
          target="_blank"
          rel="noopener noreferrer"
          className="underline underline-offset-2 hover:text-foreground"
        >
          Hyperspace PODS
        </a>
      </footer>
    </main>
  );
}

function Feature({
  icon,
  title,
  description,
}: {
  icon: string;
  title: string;
  description: string;
}) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-2xl">{icon}</span>
      <h3 className="font-semibold text-sm">{title}</h3>
      <p className="text-sm text-muted-foreground leading-relaxed">
        {description}
      </p>
    </div>
  );
}
