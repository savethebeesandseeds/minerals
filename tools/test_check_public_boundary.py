from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest


MODULE_PATH = Path(__file__).with_name("check-public-boundary.py")
SPEC = importlib.util.spec_from_file_location("check_public_boundary", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
BOUNDARY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BOUNDARY)


class PublicBoundaryTests(unittest.TestCase):
    def test_admin_password_literal_is_detected_but_placeholder_is_allowed(self) -> None:
        name = b"ADMIN_" + b"PASSWORD"
        literal = name + b"=N7p4R8t6W3x5"
        self.assertIn(
            "literal value assigned to ADMIN_PASSWORD",
            BOUNDARY.secret_reasons(literal),
        )
        self.assertEqual(
            BOUNDARY.secret_reasons(name + b"=replace-with-a-random-secret"),
            set(),
        )

    def test_historical_secret_is_found_after_removal_from_the_tip(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary)
            self.run_git(repo, "init", "--initial-branch=main")
            self.run_git(repo, "config", "user.name", "Boundary Test")
            self.run_git(repo, "config", "user.email", "boundary@example.invalid")

            token = "sk-" + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4P5q6"
            (repo / "settings.txt").write_text(
                "OPENAI_" + "API_KEY=" + token + "\n",
                encoding="utf-8",
            )
            self.run_git(repo, "add", "settings.txt")
            self.run_git(repo, "commit", "-m", "temporary secret")

            (repo / "settings.txt").write_text("safe=true\n", encoding="utf-8")
            self.run_git(repo, "add", "settings.txt")
            self.run_git(repo, "commit", "-m", "remove secret")

            self.assertEqual(
                BOUNDARY.scan_tracked_secrets(repo, BOUNDARY.tracked_paths(repo)),
                [],
            )
            findings = BOUNDARY.scan_historical_secrets(repo)
            self.assertTrue(
                any(reason == "OpenAI API key" for _, reason in findings),
                findings,
            )

    @staticmethod
    def run_git(repo: Path, *arguments: str) -> None:
        subprocess.run(
            ["git", "-c", f"safe.directory={repo.as_posix()}", *arguments],
            cwd=repo,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


if __name__ == "__main__":
    unittest.main()
