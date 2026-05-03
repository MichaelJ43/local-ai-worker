# Build from repository root: docker build -f docs-site/Dockerfile .
FROM mcr.microsoft.com/dotnet/sdk:8.0 AS build
WORKDIR /src
COPY docs-site/LocalAiWorker.Docs/ ./LocalAiWorker.Docs/
COPY docs/images/ ./docs/images/
WORKDIR /src/LocalAiWorker.Docs
RUN dotnet publish -c Release -o /app /p:UseAppHost=false

FROM mcr.microsoft.com/dotnet/aspnet:8.0 AS runtime
WORKDIR /app
COPY --from=build /app .
ENV ASPNETCORE_URLS=http://0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["dotnet", "LocalAiWorker.Docs.dll"]
