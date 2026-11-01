#!/usr/bin/env python3
"""Checks a launch-gate run's junit.xml (CI job launch-gate).

    check_junit.py <junit.xml> <environment> <port>

One suite for the Shop API with the environment as a property, the three
requests passing in order, each with its request file, sent to this
environment's server.
"""
import os
import sys
import xml.etree.ElementTree as ET

path, environment, port = sys.argv[1], sys.argv[2], sys.argv[3]
root = ET.parse(path).getroot()
assert root.tag == "testsuites", root.tag
assert (root.get("tests"), root.get("failures"), root.get("errors")) == ("3", "0", "0"), root.attrib
(suite,) = root.findall("testsuite")
assert suite.get("name") == "Shop API", suite.attrib
props = {p.get("name"): p.get("value") for p in suite.iter("property")}
assert props.get("environment") == environment, props
expected = [
    ("Log in", "shop-api/requests/auth/log-in.yaml", f"POST http://127.0.0.1:{port}/login -> 200"),
    ("List orders", "shop-api/requests/orders/list-orders.yaml", f"GET http://127.0.0.1:{port}/orders?limit=2 -> 200"),
    ("Get order", "shop-api/requests/orders/get-order.yaml", f"GET http://127.0.0.1:{port}/orders/ord-{environment.lower()}-1 -> 200"),
]
cases = suite.findall("testcase")
assert len(cases) == len(expected), [c.get("name") for c in cases]
for case, (name, file, line) in zip(cases, expected):
    assert case.get("name") == name and case.get("classname") == "Shop API", case.attrib
    assert case.get("file") == file, case.attrib
    assert os.path.isfile(file), file
    assert case.find("failure") is None and case.find("error") is None, ET.tostring(case)
    out = case.findtext("system-out") or ""
    assert out.startswith(line), out
    assert "✗" not in out, out
print(f"{path}: {environment} passed, 3 testcases with their files")
