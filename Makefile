.PHONY: build release run clean combine commit push github

build:
	cargo build

release:
	cargo build --release

release-win-gnu:
	cargo build --target x86_64-pc-windows-gnu --release

release-win-msvc:
	cargo build --target x86_64-pc-windows-msvc --release

run:
	cargo run

clean:
	cargo clean
	rm -f combined.txt

commit:
	git add -A
	git commit -m "$(msg)"

push:
	git push

github: commit push

combine:
	{ \
	  echo "=== Cargo.toml ===" && cat Cargo.toml && \
	  for f in src/*.rs; do echo "=== $$f ===" && cat "$$f"; done && \
	  echo "=== doc.md ===" && cat doc.md; \
	} > combined.txt
	@echo "combined.txt generado"
