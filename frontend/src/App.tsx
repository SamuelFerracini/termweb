import type { KeyboardEvent } from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";

type LineKind = "input" | "output" | "error";

type TerminalLine = {
  id: string;
  kind: LineKind;
  text: string;
  prompt?: string;
};

type CommandResponse = {
  output: string;
  cwd: string;
  status: "ok" | "error";
  clear: boolean;
};

const API_URL = import.meta.env.VITE_API_URL ?? "http://localhost:3000";

function App() {
  const [lines, setLines] = useState<TerminalLine[]>([]);
  const [cwd, setCwd] = useState("/");
  const [input, setInput] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const outputRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const prompt = useMemo(() => `user@termweb:${cwd}$`, [cwd]);

  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
    if (!isRunning) {
      inputRef.current?.focus();
    }
  }, [lines, isRunning]);

  const appendLine = (line: TerminalLine) => {
    setLines((prev) => [...prev, line]);
  };

  const runCommand = async (command: string) => {
    const trimmed = command.trim();
    if (!trimmed) {
      setInput("");
      return;
    }

    appendLine({
      id: crypto.randomUUID(),
      kind: "input",
      text: trimmed,
      prompt,
    });

    setInput("");
    setHistory((prev) => [...prev, trimmed]);
    setHistoryIndex(null);
    setIsRunning(true);

    try {
      const response = await fetch(`${API_URL}/api/command`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ command: trimmed }),
      });
      const data = (await response.json()) as CommandResponse;
      setCwd(data.cwd);

      if (data.clear) {
        setLines([]);
        return;
      }

      if (data.output) {
        appendLine({
          id: crypto.randomUUID(),
          kind: data.status === "error" ? "error" : "output",
          text: data.output,
        });
      }
    } catch (error) {
      appendLine({
        id: crypto.randomUUID(),
        kind: "error",
        text: "Failed to reach the server.",
      });
    } finally {
      setIsRunning(false);
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.ctrlKey && event.key.toLowerCase() === "l") {
      event.preventDefault();
      setLines([]);
      return;
    }

    if (event.key === "Enter") {
      event.preventDefault();
      void runCommand(input);
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (history.length === 0) return;
      const nextIndex =
        historyIndex === null
          ? history.length - 1
          : Math.max(0, historyIndex - 1);
      setHistoryIndex(nextIndex);
      setInput(history[nextIndex]);
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (history.length === 0) return;
      if (historyIndex === null) return;
      const nextIndex = historyIndex + 1;
      if (nextIndex >= history.length) {
        setHistoryIndex(null);
        setInput("");
      } else {
        setHistoryIndex(nextIndex);
        setInput(history[nextIndex]);
      }
    }
  };

  return (
    <div className="dark min-h-screen bg-background text-foreground">
      <div className="mx-auto flex min-h-screen max-w-5xl flex-col px-6 py-10">
        <Card className="mt-6 flex-1 border-border bg-black/50 shadow-sm">
          <div
            className="relative flex h-full flex-col"
            onClick={() => inputRef.current?.focus()}
          >
            <div
              ref={outputRef}
              className="flex-1 overflow-y-auto px-4 py-4 font-mono text-sm text-foreground"
            >
              {lines.length === 0 ? (
                <div className="text-muted-foreground">
                  Type <span className="text-foreground">help</span> to see
                  available commands.
                </div>
              ) : (
                lines.map((line) => (
                  <div
                    key={line.id}
                    className="whitespace-pre-wrap leading-relaxed"
                  >
                    {line.kind === "input" ? (
                      <>
                        <span className="text-emerald-400">{line.prompt}</span>{" "}
                        <span className="text-foreground">{line.text}</span>
                      </>
                    ) : (
                      <span
                        className={
                          line.kind === "error"
                            ? "text-red-400"
                            : "text-foreground"
                        }
                      >
                        {line.text}
                      </span>
                    )}
                  </div>
                ))
              )}
            </div>

            <div className="sticky bottom-0 flex items-center gap-2 border-t border-border/60 bg-black/90 px-4 py-3 font-mono text-sm">
              <span className="text-emerald-400">{prompt}</span>
              <Input
                ref={inputRef}
                value={input}
                onChange={(event) => setInput(event.target.value)}
                onKeyDown={handleKeyDown}
                className="h-8 flex-1 border-none bg-transparent px-0 text-sm text-foreground focus-visible:ring-0 focus-visible:ring-offset-0"
                placeholder="Type a command and hit Enter"
                disabled={isRunning}
                autoFocus
              />
            </div>
          </div>
        </Card>

        <footer className="mt-4 text-xs text-muted-foreground">
          Tip: Use ↑ / ↓ for history, Ctrl + L to clear.
        </footer>
      </div>
    </div>
  );
}

export default App;
