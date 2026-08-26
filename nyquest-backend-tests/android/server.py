from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class RuntimeHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/runtime":
            self.send_error(404)
            return

        body = b"nyquest-runtime-ok"
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Content-Type", "text/plain")
        self.send_header("X-Nyquest-Runtime", "ok")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        print(format % args, flush=True)


ThreadingHTTPServer(("0.0.0.0", 8765), RuntimeHandler).serve_forever()
