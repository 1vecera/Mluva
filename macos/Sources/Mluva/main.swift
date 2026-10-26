import MluvaKit
print(try CoreBridge.shared.call("key.validate", ["key": "probe"]))
