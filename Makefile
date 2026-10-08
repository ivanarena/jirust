.PHONY: build test check install
build:
	go build -trimpath -ldflags='-s -w' -o bin/jiraui ./cmd/jiraui
test:
	go test ./...
check:
	go vet ./...
	go test -race ./...
install: build
	install -Dm755 bin/jiraui "$(HOME)/.local/bin/jiraui"
