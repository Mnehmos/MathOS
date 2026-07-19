.PHONY: install test adversarial demo validate-demo check reset-demo

PYTHON := .venv/bin/python
UV_CACHE_DIR ?= .cache/uv
PYTHONWARNINGS ?= error::ResourceWarning

install:
	UV_CACHE_DIR=$(UV_CACHE_DIR) uv venv --python 3.12 --system-site-packages --allow-existing
	UV_CACHE_DIR=$(UV_CACHE_DIR) uv pip install --python $(PYTHON) --no-build-isolation -e .

test:
	PYTHONWARNINGS=$(PYTHONWARNINGS) $(PYTHON) -m unittest discover -s tests -v

adversarial:
	PYTHONWARNINGS=$(PYTHONWARNINGS) $(PYTHON) -m unittest -v tests.test_adversarial

demo:
	$(PYTHON) -m mathos demo --workspace .mathos-demo --reset

validate-demo:
	@for trajectory in .mathos-demo/exports/*.json; do \
		$(PYTHON) -m mathos validate-export --input "$$trajectory" > /dev/null || exit 1; \
	done

check:
	$(PYTHON) -m compileall -q src tests
	PYTHONWARNINGS=$(PYTHONWARNINGS) $(PYTHON) -m unittest discover -s tests -v

reset-demo:
	$(PYTHON) -m mathos demo --workspace .mathos-demo --reset
