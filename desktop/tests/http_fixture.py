"""桌面端到端测试用公开 API 服务；合成数据只用于验收，应用不会调用它。"""

import json
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit


class PublicMarket(BaseHTTPRequestHandler):
    """只处理测试所需公开端点；记录认证/周期错误，供主流程汇总验收。"""

    errors = []  # 本次测试发现的协议问题；服务结束后主线程统一报告。

    def do_GET(self):
        """输入：self 的公开 GET 路径/请求头；返回：通过 HTTP 写出确定性 JSON，函数返回 None。"""
        path = urlsplit(self.path)  # 分离端点路径与查询参数，避免查询影响路由。
        if self.headers.get("X-MBX-APIKEY"):
            self.errors.append("历史请求不应携带账户密钥")
        if path.path.endswith("/time"):
            value = {"serverTime": 2_000_000_000_000}
        elif path.path.endswith("/ticker/price"):
            value = {"symbol": "BTCUSDT", "price": "84000"}
        elif path.path.endswith("/exchangeInfo"):
            value = {"symbols": [{"symbol": "BTCUSDT", "status": "TRADING",
                "quoteAsset": "USDT", "isSpotTradingAllowed": True, "filters": [
                {"filterType": "PRICE_FILTER", "tickSize": "0.10", "minPrice": "0.10", "maxPrice": "1000000"},
                {"filterType": "LOT_SIZE", "stepSize": "0.00001", "minQty": "0.00001", "maxQty": "100"},
                {"filterType": "MIN_NOTIONAL", "minNotional": "15"}]}]}
        elif path.path.endswith("/klines"):
            query = parse_qs(path.query)  # 验证周期与已收盘截止时间被显式传递。
            if query.get("interval") != ["1d"] or "endTime" not in query:
                self.errors.append("历史请求未指定周期或收盘截止时间")
            value = []  # 合成 OHLC 仅作为验收输入，末根设极端值检查收盘筛选。
            for i in range(101):  # 100 根已收盘日线加 1 根未收盘日线。
                start = 2_000_000_000_000 - (100 - i) * 86_400_000  # 固定模拟时间，确保测试可重现。
                value.append([start, "84000", "999999" if i == 100 else "85000",
                    "83000", "84000", "10", start + 86_400_000 - 1])
        else:
            self.errors.append(f"不允许的公开端点：{path.path}")
            value = {}
        body = json.dumps(value).encode()  # value 是当前端点响应，body 是 UTF-8 JSON 字节。
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _format, *_args):
        """输入：服务器日志参数；返回：无，避免用逐请求日志淹没验收结果。"""


def main():
    """输入：桥接验收程序及包内后端路径；返回：真实子进程的退出码，清理本地服务。"""
    with ThreadingHTTPServer(("127.0.0.1", 0), PublicMarket) as server:  # 回环随机端口，离开上下文即关闭。
        thread = threading.Thread(target=server.serve_forever, daemon=True)  # 子进程验收期间持续服务 HTTP。
        thread.start()
        try:
            api = f"http://127.0.0.1:{server.server_port}"  # 临时公开 API 模拟地址，追加给桥接入口。
            result = subprocess.run([*sys.argv[1:], api], check=False)  # 参数数组直接执行，不经过 Shell。
        finally:
            server.shutdown()
            thread.join(timeout=5)
    if PublicMarket.errors:
        print("\n".join(PublicMarket.errors), file=sys.stderr)
        return 1
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
