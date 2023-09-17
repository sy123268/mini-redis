#![feature(impl_trait_in_assoc_type)]

use std::{sync::Mutex, collections::HashMap, process, io::Write};
use anyhow::{Error, Ok};


fn connect_to_aof(s: &S, req: &volo_gen::mini::redis::RedisRequest) -> Result<(), std::io::Error> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&s.aof_file)?;
    let operation_str = redis_operation(req);
    file.write_all(operation_str.as_bytes())?;
    file.write_all(b"\n")?;
    std::result::Result::Ok(())
}


fn redis_operation(req: &volo_gen::mini::redis::RedisRequest) -> String {
    match req.request_type {
        volo_gen::mini::redis::RequestType::Set => {
            if let Some(key) = &req.key {
                if let Some(value) = &req.value {
                    return format!("SET {} {}\n", key[0], value);
                }
            }
        }
        volo_gen::mini::redis::RequestType::Del => {
            if let Some(key) = &req.key {
                return format!("DEL {}\n", key.join(" "));
            }
        }
        _ => {}
    }
    String::new()
}

pub struct S {
    pub map: Mutex<HashMap<String, String>>,
    pub aof_file: String,
}

#[volo::async_trait]
impl volo_gen::mini::redis::RedisService for S {
    async fn redis_command(
        &self,
        req: volo_gen::mini::redis::RedisRequest,
    ) -> ::core::result::Result<volo_gen::mini::redis::RedisResponse, ::volo_thrift::AnyhowError>
    {
        match req.request_type {
            volo_gen::mini::redis::RequestType::Ping => {
                return Ok(volo_gen::mini::redis::RedisResponse {
                    value: Some(format!("PONG").into()),
                    response_type: volo_gen::mini::redis::ResponseType::Value,
                });
            }
            volo_gen::mini::redis::RequestType::Set => {
                let _ = self.map.lock().unwrap().insert(req.clone().key.unwrap().get(0).unwrap().to_string(), req.clone().value.unwrap().to_string(),);
                if let Err(err) = connect_to_aof(&self, &req) {
                    tracing::error!("Failed to append to AOF file: {:?}", err);
                }
                return Ok(volo_gen::mini::redis::RedisResponse {
                    value: Some(format!("OK!",).into()),
                    response_type: volo_gen::mini::redis::ResponseType::Ok,
                });
            }
            volo_gen::mini::redis::RequestType::Get => {
                if let Some(str) = self.map.lock().unwrap().get(&req.key.unwrap().get(0).unwrap().to_string())
                {
                    return Ok(volo_gen::mini::redis::RedisResponse {
                        value: Some(str.clone().into()),
                        response_type: volo_gen::mini::redis::ResponseType::Value,
                    });
                } else {
                    return Ok(volo_gen::mini::redis::RedisResponse {
                        value: Some(format!("NULL!").into()),
                        response_type: volo_gen::mini::redis::ResponseType::Value,
                    });
                }
            }
            volo_gen::mini::redis::RequestType::Del => {
                let mut count = 0;
                for i in req.clone().key.unwrap() {
                    if let Some(_) = self.map.lock().unwrap().remove(&i.to_string()) {
                        count += 1;
                    }
                }
                if let Err(err) = connect_to_aof(&self, &req) {
                    tracing::error!("Failed to append to AOF file: {:?}", err);
                }
                return Ok(volo_gen::mini::redis::RedisResponse {
                    value: Some(format!("{}", count).into()),
                    response_type: volo_gen::mini::redis::ResponseType::Value,
                });
            }
            volo_gen::mini::redis::RequestType::Exit => {
                process::exit(0);
            }
            _ => {}
        }
        Ok(Default::default())
    }
}

#[derive(Clone)]
pub struct LogService<S>(S);


#[volo::service]
impl<Cx, Req, S> volo::Service<Cx, Req> for LogService<S>
where
    Req: std::fmt::Debug + Send + 'static,
    S: Send + 'static + volo::Service<Cx, Req> + Sync,
    S::Response: std::fmt::Debug,
    S::Error: std::fmt::Debug + From<Error>,
    Cx: Send + 'static,
{
    async fn call(&self, cx: &mut Cx, req: Req) -> Result<S::Response, S::Error> {
        let now = std::time::Instant::now();
        tracing::debug!("Received request {:?}", &req);
        let info = format!("{:?}", &req);
        println!("{}", info);
        if info.contains("Illegal") {
            return Err(S::Error::from(Error::msg("Illegal！")));
        }
        let resp = self.0.call(cx, req).await;
        tracing::debug!("Sent response {:?}", &resp);
        tracing::info!("Request took {}ms", now.elapsed().as_millis());
        resp
    }
}
pub struct LogLayer;
impl<S> volo::Layer<S> for LogLayer {
    type Service = LogService<S>;
    fn layer(self, inner: S) -> Self::Service {
        LogService(inner)
    }
}