use std::sync::Arc;

use jni::objects::{Global, JObject};
use jni::{Env, JavaVM};
use nyquest_interface::client::ClientOptions;
use nyquest_interface::Result;

use crate::bindings::android::net::http::HttpEngine;
use crate::bindings::jni_init;
#[cfg(feature = "blocking")]
use crate::blocking::AndroidBlockingClient;
use crate::error::unsupported;
#[cfg(feature = "async")]
use crate::r#async::AndroidAsyncClient;

#[derive(Clone)]
pub struct AndroidBackend {
    core: Arc<BackendCore>,
}

pub(crate) struct BackendCore {
    pub(crate) vm: Arc<JavaVM>,
    pub(crate) engine: Global<HttpEngine<'static>>,
    pub(crate) executor: Global<JObject<'static>>,
}

impl AndroidBackend {
    pub fn new(
        env: &mut Env<'_>,
        engine: &JObject<'_>,
        executor: &JObject<'_>,
    ) -> jni::errors::Result<Self> {
        jni_init(env, &jni::refs::LoaderContext::FromObject(engine))?;
        Ok(Self {
            core: Arc::new(BackendCore {
                vm: Arc::new(env.get_java_vm()?),
                engine: env.new_cast_global_ref::<HttpEngine>(engine)?,
                executor: env.new_global_ref(executor)?,
            }),
        })
    }

    #[cfg(feature = "async")]
    pub fn create_async_client(&self, options: ClientOptions) -> Result<AndroidAsyncClient> {
        validate_options(&options)?;
        Ok(AndroidAsyncClient {
            core: Arc::clone(&self.core),
            options,
        })
    }

    #[cfg(feature = "blocking")]
    pub fn create_blocking_client(&self, options: ClientOptions) -> Result<AndroidBlockingClient> {
        validate_options(&options)?;
        Ok(AndroidBlockingClient {
            core: Arc::clone(&self.core),
            options,
        })
    }
}

fn validate_options(options: &ClientOptions) -> Result<()> {
    if options.ignore_certificate_errors {
        return Err(unsupported(
            "certificate policy must be configured when the Android engine is created",
        ));
    }
    Ok(())
}
