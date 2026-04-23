# Build stage
FROM golang:1.26-bookworm AS builder

WORKDIR /build
COPY go.mod go.sum ./
RUN go mod download
COPY cmd/ cmd/
COPY internal/ internal/

RUN CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -o /out/api-gateway ./cmd/api-gateway
RUN CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -o /out/node-agent ./cmd/node-agent

# api-gateway runtime
FROM gcr.io/distroless/base-debian12 AS api-gateway
COPY --from=builder /out/api-gateway /usr/local/bin/api-gateway
EXPOSE 8080
CMD ["/usr/local/bin/api-gateway"]

# node-agent runtime
FROM gcr.io/distroless/base-debian12 AS node-agent
COPY --from=builder /out/node-agent /usr/local/bin/node-agent
EXPOSE 8181
CMD ["/usr/local/bin/node-agent"]
