use clap::Parser;
use ipp_message_util::inbound::Inbound;
use std::io::{self};

use axum::{
  Router,
  body::Body,
  debug_handler,
  extract::{Request, State},
  response::IntoResponse,
  routing::post,
};
use futures::TryStreamExt;
use ipp_message::{Limits, Message, edit::Edit};
use reqwest::{Client, StatusCode, header::CONTENT_TYPE};

const IPP_CONTENT_TYPE: &str = "application/ipp";

#[derive(Parser, Clone, Debug)]
struct Args {
  /// The upstream URL to forward requests to.
  #[arg(short, long)]
  upstream: String,
}

fn naive_rewriter(_input: &Message, _edit: &mut Edit) {}

#[debug_handler]
async fn proxy_handler(
  State(state): State<Args>,
  req: Request,
) -> Result<impl IntoResponse, impl IntoResponse> {
  // make sure content-type is IPP
  match req.headers().get(CONTENT_TYPE) {
    Some(content_type)
      if matches!(content_type.to_str(), Ok(IPP_CONTENT_TYPE)) => {}
    Some(_) => {
      return Err((
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "Unsupported content type",
      ));
    }
    None => return Err((StatusCode::BAD_REQUEST, "No content type specified")),
  }

  // form the inbound stream from the request body
  let inbound =
    Inbound::read(req.into_body().into_data_stream(), Limits::default()).await;

  // if the inbound stream is interrupted, fail the request
  let Ok(inbound) = inbound else {
    return Err((StatusCode::BAD_REQUEST, "Request terminated early"));
  };

  println!("----- inbound message -------------");
  println!("{:?}", inbound.message());
  println!("----- end of inbound message ------");

  // split the message and body if the message is propery terminated
  let Ok(mut split) = inbound.split() else {
    return Err((StatusCode::BAD_REQUEST, "unterminated IPP message"));
  };

  split
    .edit(naive_rewriter)
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "editing failed"))?;

  let outbound = split.into_outbound();

  let client = Client::new();
  let resp = client
    .post(&state.upstream)
    .header(CONTENT_TYPE, IPP_CONTENT_TYPE)
    .body(reqwest::Body::wrap_stream(Box::pin(
      outbound.map_err(Box::new),
    )))
    .send()
    .await
    .unwrap();
  println!("sent!");

  println!("---- receiving reply message -------");
  let resp_body = resp.bytes_stream().map_err(io::Error::other);

  let reply_inbound = Inbound::read(resp_body, Limits::default()).await;

  let Ok(reply_inbound) = reply_inbound else {
    return Err((
      StatusCode::BAD_GATEWAY,
      "reply from upstream was interrupted",
    ));
  };

  println!("----- outbound message ------------");
  println!("{:?}", reply_inbound.message());
  println!("----- end of outbound message -----");

  Ok(Body::from_stream(reply_inbound.into_verbatim()))
}

#[tokio::main]
async fn main() {
  let args = Args::parse();

  let router = Router::new()
    .route("/proxy", post(proxy_handler))
    .with_state(args);

  let listener = tokio::net::TcpListener::bind("0.0.0.0:6311")
    .await
    .expect("failed to bind listener");

  axum::serve(listener, router)
    .await
    .expect("failed to serve")
}
