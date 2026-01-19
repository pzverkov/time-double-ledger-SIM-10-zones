FROM golang:1.25.5 as build
WORKDIR /src
COPY go.mod ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -o /out/sim-go ./cmd/sim-go

FROM gcr.io/distroless/static:nonroot
WORKDIR /
COPY --from=build /out/sim-go /sim-go
USER nonroot:nonroot
EXPOSE 8080
ENTRYPOINT ["/sim-go"]
