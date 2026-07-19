from pathlib import Path
import shutil
import subprocess

from .canonical import hash_json
from .models import VerificationOutcome, VerificationResult


class LeanSubprocessVerifier:
    """Fail-closed adapter for a future Lean trust boundary."""

    name = "lean-subprocess"
    version = "0.1.0"

    def _result(self, outcome: VerificationOutcome, details: dict) -> VerificationResult:
        evidence = {
            "outcome": outcome.value,
            "verifier": self.name,
            "verifier_version": self.version,
            "details": details,
        }
        return VerificationResult(
            outcome,
            self.name,
            self.version,
            details,
            hash_json(evidence),
        )

    def verify_file(self, path: str | Path, *, timeout: int = 60) -> VerificationResult:
        source = Path(path)
        lake = shutil.which("lake")
        lean = shutil.which("lean")
        if lake:
            command = [lake, "env", "lean", str(source)]
        elif lean:
            command = [lean, str(source)]
        else:
            return self._result(
                VerificationOutcome.UNKNOWN,
                {"reason": "lean_toolchain_unavailable", "source": str(source)},
            )
        try:
            completed = subprocess.run(
                command,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
        except (subprocess.TimeoutExpired, OSError) as error:
            return self._result(
                VerificationOutcome.UNKNOWN,
                {
                    "reason": "lean_execution_failed",
                    "source": str(source),
                    "command": command,
                    "error_type": type(error).__name__,
                },
            )
        details = {
            "source": str(source),
            "command": command,
            "returncode": completed.returncode,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
        }
        outcome = (
            VerificationOutcome.PROVED
            if completed.returncode == 0
            else VerificationOutcome.REJECTED
        )
        return self._result(outcome, details)
