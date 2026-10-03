"""Regression checks for the RPC gate's read-only and cutoff boundary."""
import json
import unittest
from unittest.mock import patch

from run_sdk_reference import EXCHANGE, Gate, failure_reason


def request(method, params):
    return {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}


class GateTests(unittest.TestCase):
    def test_failure_export_uses_only_fixed_codes_and_discards_private_text(self):
        self.assertEqual(failure_reason(
            b"Reference verification failed: as-of mark is stale (60-second application limit) private provider text"
        ), "canonical_mark_stale")
        self.assertEqual(failure_reason(b"private provider text"), "unclassified_local_failure")
        self.assertEqual(failure_reason(b"funding cumulative sums are discontinuous"),
                         "unclassified_local_failure")
        self.assertEqual(failure_reason(b"Reference verification failed: " + b"x" * 65536),
                         "unclassified_local_failure")

    def test_only_pinned_read_operations_are_allowed(self):
        gate = Gate(100, 2, "https://rpc.example.invalid")
        gate.validate(request("eth_chainId", []))
        gate.validate(request("eth_getBlockByNumber", ["0x64", False]))
        gate.validate(request("eth_call", [{"to": EXCHANGE, "data": "0x12345678"}, "0x64"]))
        for item in [
            request("eth_sendRawTransaction", ["0x00"]),
            request("personal_sign", ["0x00", EXCHANGE]),
            request("eth_call", [{"to": EXCHANGE, "data": "0x12345678"}, "latest"]),
            request("eth_call", [{"to": EXCHANGE, "data": "0x12345678"}, "0x64", {}]),
            request("eth_call", [{"to": "0x" + "0" * 40, "data": "0x12345678"}, "0x64"]),
            request("eth_getBlockByNumber", ["0x64", True]),
        ]:
            with self.subTest(item=item), self.assertRaises(ValueError):
                gate.validate(item)

    def test_batch_is_denied_before_network_and_allowance_counts_failed_attempts(self):
        gate = Gate(100, 1, "https://rpc.example.invalid")
        good = request("eth_chainId", [])
        bad = request("eth_sendTransaction", [])
        with patch("urllib.request.urlopen", side_effect=OSError("private provider text")) as network:
            with self.assertRaises(ValueError):
                gate.forward(json.dumps([good, bad]).encode())
            network.assert_not_called()
            with self.assertRaisesRegex(ValueError, "^rpc_unavailable$"):
                gate.forward(json.dumps(good).encode())
            self.assertEqual(network.call_count, 1)
            with self.assertRaisesRegex(ValueError, "request_allowance_exhausted"):
                gate.forward(json.dumps(good).encode())
            self.assertEqual(network.call_count, 1)
        self.assertEqual(sum(gate.methods.values()), 1)
        self.assertEqual(gate.failed, 1)


if __name__ == "__main__":
    unittest.main()
