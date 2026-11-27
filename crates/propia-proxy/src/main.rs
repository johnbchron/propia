#![feature(string_replace_in_place)]

use clap::Parser;
use ipp_message_util::inbound::Inbound;
use std::io::{self};
use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use axum::{
  Router,
  body::Body,
  debug_handler,
  extract::{Path, Request, State},
  response::IntoResponse,
  routing::post,
};
use futures::TryStreamExt;
use ipp_message::{Limits, Message, edit::Edit};
use reqwest::{
  Client, StatusCode, Url,
  header::{CONTENT_LENGTH, CONTENT_TYPE},
};

const IPP_CONTENT_TYPE: &str = "application/ipp";

#[derive(Parser, Clone, Debug)]
struct Args {
  /// The port that the proxy listens on.
  #[arg(short = 'p', long, default_value = "6311")]
  listen_port: u16,
  /// The upstream URL to forward requests to.
  #[arg(short, long)]
  upstream: String,
}

#[derive(Clone, Debug)]
struct AppState {
  client: Client,
  upstream_url: Url,
}

fn naive_rewriter(_input: &Message, _edit: &mut Edit) {}

#[debug_handler]
async fn proxy_handler(
  State(state): State<AppState>,
  path: Option<Path<String>>,
  req: Request,
) -> Result<impl IntoResponse, impl IntoResponse> {
  // make sure content-type is IPP
  match req.headers().get(CONTENT_TYPE) {
    Some(content_type)
      if matches!(content_type.to_str(), Ok(IPP_CONTENT_TYPE)) => {}
    Some(_) => {
      tracing::error!("unsupported content type");
      return Err((
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "Unsupported content type",
      ));
    }
    None => {
      tracing::error!("no content type specified");
      return Err((StatusCode::BAD_REQUEST, "No content type specified"));
    }
  }

  let inbound_content_length = req
    .headers()
    .get(CONTENT_LENGTH)
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.parse::<u64>().ok());

  // form the inbound stream from the request body
  let inbound =
    Inbound::read(req.into_body().into_data_stream(), Limits::default()).await;

  // if the inbound stream is interrupted, fail the request
  let Ok(inbound) = inbound else {
    tracing::error!("inbound request terminated early");
    return Err((StatusCode::BAD_REQUEST, "Request terminated early"));
  };

  // assign content length
  let inbound = inbound.with_len(inbound_content_length);

  tracing::info!("got inbound message: {:?}", inbound.message());

  // split and edit if the message is propery terminated, otherwise just send
  let outbound = match inbound.split() {
    Ok(mut split) => {
      // edit the message with the rewriter
      split.edit(naive_rewriter).map_err(|_| {
        tracing::error!("editing failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "editing failed")
      })?;

      // prep for outbound stream
      split.into_outbound()
    }
    Err(inbound) => {
      // pass along without editing
      tracing::error!("failed to split message from document");
      inbound.into_verbatim()
    }
  };

  // set the upstream with the current path
  let mut upstream = state.upstream_url.clone();
  upstream.set_path(path.as_ref().map(|p| &*p.0).unwrap_or("/"));

  let reply = state
    .client
    .post(upstream)
    .header(CONTENT_TYPE, IPP_CONTENT_TYPE)
    .body(reqwest::Body::wrap_stream(Box::pin(
      outbound.map_err(Box::new),
    )))
    .send()
    .await
    .map_err(|error| {
      tracing::error!(%error, "failed to send to upstream");
      (StatusCode::BAD_GATEWAY, "failed to send to upstream")
    })?;

  let reply_status = reply.status();
  let reply_headers = reply.headers().clone();
  let reply_body = reply.bytes_stream().map_err(io::Error::other);

  let reply_inbound = Inbound::read(reply_body, Limits::default()).await;

  let Ok(reply_inbound) = reply_inbound else {
    tracing::error!("reply from upstream terminated early");
    return Err((
      StatusCode::BAD_GATEWAY,
      "reply from upstream was interrupted",
    ));
  };

  tracing::info!("got reply message: {:?}", reply_inbound.message());

  Ok((
    reply_status,
    reply_headers,
    Body::from_stream(reply_inbound.into_verbatim()),
  ))
}

#[tokio::main]
async fn main() {
  tracing_subscriber::registry()
    .with(fmt::layer())
    .with(
      EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy(),
    )
    .init();

  let Args {
    listen_port,
    mut upstream,
  } = Args::parse();

  // replace scheme; covers both ipp and ipps
  if upstream.starts_with("ipp") {
    upstream.replace_first("ipp", "http");
  }

  let upstream_url: Url =
    upstream.parse().expect("failed to parse upstream URL");
  if upstream_url.path() != "/" {
    panic!(
      "upstream url should not have a path set: {}",
      upstream_url.path()
    );
  }

  let listen_addr = format!("0.0.0.0:{listen_port}");
  tracing::info!("proxying traffic at {listen_addr} to: {upstream_url}");

  let app_state = AppState {
    client: Client::new(),
    upstream_url,
  };

  let router = Router::new()
    .route("/", post(proxy_handler))
    .route("/{*wildcard}", post(proxy_handler))
    .with_state(app_state);

  let listener = tokio::net::TcpListener::bind(listen_addr)
    .await
    .expect("failed to bind listener");

  axum::serve(listener, router)
    .await
    .expect("failed to serve")
}
