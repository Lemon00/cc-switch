from pathlib import Path

p = Path(r"crates/cc-switch-core/src/proxy/providers/streaming_gemini.rs")
t = p.read_text(encoding="utf-8")
needle = """                        if data.trim() == "[DONE]" {
                            // Exit both the SSE-block loop and the outer byte
                            // stream loop so we don't stall on keepalives after DONE.
                            break;
                        }"""
repl = """                        if data.trim() == "[DONE]" {
                            // Exit both the SSE-block loop and the outer byte
                            // stream loop so we don't stall on keepalives after DONE.
                            stream_done = true;
                            break;
                        }"""
if needle not in t:
    raise SystemExit("needle not found")
p.write_text(t.replace(needle, repl, 1), encoding="utf-8")
print("ok")