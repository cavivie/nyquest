use std::sync::Arc;

use jni::objects::{GlobalRef, JObject};
use jni::{JNIEnv, JavaVM};
use nyquest_interface::client::ClientOptions;
use nyquest_interface::Result;

#[cfg(feature = "blocking")]
use crate::blocking::AndroidBlockingClient;
use crate::callback::register_callback_natives;
use crate::error::unsupported;
#[cfg(feature = "async")]
use crate::r#async::AndroidAsyncClient;

/// Java API details needed by the Android request driver.
#[derive(Clone, Copy)]
pub struct BackendBindings {
    pub name: &'static str,
    pub callback_class: &'static str,
    pub callback_constructor: &'static str,
    pub engine_builder_signature: &'static str,
    pub callback_before_executor: bool,
    pub builder_class: &'static str,
    pub request_class: &'static str,
    pub upload_provider_class: &'static str,
    pub upload_provider_signature: &'static str,
    pub disable_cache_method: &'static str,
    pub disable_cache_signature: &'static str,
    pub disable_cache_takes_boolean: bool,
}

#[derive(Clone)]
pub struct AndroidBackend {
    core: Arc<BackendCore>,
}

pub(crate) struct BackendCore {
    pub(crate) vm: Arc<JavaVM>,
    pub(crate) engine: GlobalRef,
    pub(crate) executor: GlobalRef,
    pub(crate) callback_class: GlobalRef,
    pub(crate) upload_provider_class: GlobalRef,
    pub(crate) bindings: BackendBindings,
}

impl AndroidBackend {
    pub fn new(
        env: &mut JNIEnv<'_>,
        engine: &JObject<'_>,
        executor: &JObject<'_>,
        bindings: BackendBindings,
    ) -> jni::errors::Result<Self> {
        let callback_class = env.find_class(bindings.callback_class)?;
        register_callback_natives(env, &callback_class)?;
        let callback_class = env.new_global_ref(callback_class)?;
        let upload_provider_class = env.find_class(bindings.upload_provider_class)?;
        let upload_provider_class = env.new_global_ref(upload_provider_class)?;
        Ok(Self {
            core: Arc::new(BackendCore {
                vm: Arc::new(env.get_java_vm()?),
                engine: env.new_global_ref(engine)?,
                executor: env.new_global_ref(executor)?,
                callback_class,
                upload_provider_class,
                bindings,
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
