FROM golang:1.26 as build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
# Build metadata injected via ldflags; the version itself stays in version.go.
ARG BUILD_COMMIT=unknown
ARG BUILD_DATE=unknown
RUN CGO_ENABLED=0 GOOS=linux go build \
      -ldflags "-X 'time-ledger-sim/go/internal/web.buildCommit=${BUILD_COMMIT}' -X 'time-ledger-sim/go/internal/web.buildDate=${BUILD_DATE}'" \
      -o /out/sim-go ./cmd/sim-go

FROM gcr.io/distroless/static:nonroot
WORKDIR /
COPY --from=build /out/sim-go /sim-go
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/sim-go"]
