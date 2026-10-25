#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""本地 TLS 代理：让只支持 HTTPS 的客户端（如 CodeBuddy）也能走明文隧道。

链路：
    CodeBuddy --https://127.0.0.1:3001--> 本代理（TLS 终止）--http--> 127.0.0.1:3000（GP 隧道）--> 服务器

同时兼容明文 HTTP：首字节不是 TLS 握手（0x16）时按明文直接透传。

用法：
    python tools/tunnel_tls_proxy.py                 # 监听 3001，转发到 3000
    python tools/tunnel_tls_proxy.py --port 3001 --upstream 3000

证书：tools/.certs/cert.pem + key.pem（自签，SAN=IP:127.0.0.1,DNS:localhost）
客户端若校验证书，需要把 cert.pem 导入「受信任的根证书颁发机构」。
"""

import argparse
import socket
import ssl
import sys
import threading

if sys.stdout and hasattr(sys.stdout, "reconfigure"):
    try:
        sys.stdout.reconfigure(encoding="utf-8")
    except Exception:  # noqa: BLE001
        pass


def pipe(a, b):
    try:
        while True:
            d = a.recv(65536)
            if not d:
                break
            b.sendall(d)
    except Exception:  # noqa: BLE001
        pass
    finally:
        for s in (a, b):
            try:
                s.close()
            except Exception:  # noqa: BLE001
                pass


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=3001)
    ap.add_argument("--upstream", type=int, default=3000)
    ap.add_argument("--cert", default="tools/.certs/cert.pem")
    ap.add_argument("--key", default="tools/.certs/key.pem")
    args = ap.parse_args()

    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(args.cert, args.key)

    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", args.port))
    srv.listen(50)
    print(f"[ok] TLS/HTTP proxy 127.0.0.1:{args.port} -> 127.0.0.1:{args.upstream}", flush=True)

    def handle(c):
        conn = None
        try:
            first = c.recv(1, socket.MSG_PEEK)
            if first[:1] == b"\x16":
                conn = ctx.wrap_socket(c, server_side=True)
            else:
                conn = c
        except Exception as e:  # noqa: BLE001
            print("[tls] handshake fail:", e, flush=True)
            try:
                c.close()
            except Exception:  # noqa: BLE001
                pass
            return
        try:
            up = socket.create_connection(("127.0.0.1", args.upstream), timeout=10)
        except Exception as e:  # noqa: BLE001
            print("[up] connect fail:", e, flush=True)
            try:
                conn.close()
            except Exception:  # noqa: BLE001
                pass
            return
        print("[req] new connection", flush=True)
        threading.Thread(target=pipe, args=(conn, up), daemon=True).start()
        threading.Thread(target=pipe, args=(up, conn), daemon=True).start()

    while True:
        c, _addr = srv.accept()
        threading.Thread(target=handle, args=(c,), daemon=True).start()


if __name__ == "__main__":
    main()
