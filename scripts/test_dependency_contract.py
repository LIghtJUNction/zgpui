"""Reject dependency/feature changes that would silently weaken the portable core boundary."""
import copy
import unittest

from check_dependency_contract import validate_metadata


BASE = {
    "packages": [
        {"name": "zgpui", "id": "core", "features": {
            "default": [], "std": [], "native": ["std"],
            "linux-software": ["native"], "bridge": ["std", "dep:zbridge"]},
         "dependencies": [
             {"name": "zbridge", "kind": None, "target": None, "rename": None,
              "optional": True, "req": "*", "source": "git+https://github.com/LIghtJUNction/zbridge.git"},
             {"name": "serde_json", "kind": "dev", "optional": False,
              "source": "registry+https://github.com/rust-lang/crates.io-index"}]},
        {"name": "zgpui-raster", "id": "raster", "dependencies": [], "publish": []},
    ],
    "workspace_default_members": ["core"],
}


class DependencyContractTests(unittest.TestCase):
    def test_explicit_optional_std_bridge_is_accepted(self):
        validate_metadata(copy.deepcopy(BASE))

    def test_default_or_std_cannot_enable_bridge(self):
        for feature in ("default", "std", "native", "linux-software"):
            data = copy.deepcopy(BASE)
            data["packages"][0]["features"][feature].append("dep:zbridge")
            with self.assertRaises(ValueError):
                validate_metadata(data)

    def test_bridge_requires_std(self):
        data = copy.deepcopy(BASE)
        data["packages"][0]["features"]["bridge"] = ["dep:zbridge"]
        with self.assertRaises(ValueError):
            validate_metadata(data)

    def test_nonoptional_target_specific_or_selected_git_dependency_is_rejected(self):
        for field, value in (("optional", False), ("target", "cfg(windows)"),
                             ("req", "=0.1.0"), ("kind", "build"),
                             ("source", "git+https://github.com/LIghtJUNction/zbridge.git?tag=v0.1.0")):
            data = copy.deepcopy(BASE)
            data["packages"][0]["dependencies"][0][field] = value
            with self.assertRaises(ValueError):
                validate_metadata(data)

    def test_test_helper_cannot_become_a_runtime_dependency(self):
        data = copy.deepcopy(BASE)
        data["packages"][0]["dependencies"][1]["kind"] = None
        with self.assertRaises(ValueError):
            validate_metadata(data)

    def test_raster_cannot_gain_dependencies_or_publication(self):
        for field, value in (("dependencies", [BASE["packages"][0]["dependencies"][0]]),
                             ("publish", None)):
            data = copy.deepcopy(BASE)
            data["packages"][1][field] = value
            with self.assertRaises(ValueError):
                validate_metadata(data)

    def test_workspace_default_cannot_expand_to_raster(self):
        data = copy.deepcopy(BASE)
        data["workspace_default_members"].append("raster")
        with self.assertRaises(ValueError):
            validate_metadata(data)


if __name__ == "__main__":
    unittest.main()
