// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a
// copy of the MPL was not distributed with this file, You can obtain one at
// http://mozilla.org/MPL/2.0/. The original code is copyright (c) rasheed.

// The open.mp side of pawn-json: a component that registers the Rust natives with every
// script the Pawn component loads. The entry point lives on the Rust side (src/lib.rs)
// and passes its functions in here, so the library exports nothing but
// ComponentEntryPoint.

#include <sdk.hpp>
#include <Server/Components/Pawn/pawn.hpp>

// The Rust functions this shell calls. It must match `Api` in src/lib.rs field for field.
struct PjApi
{
	const AMX_NATIVE_INFO* (*natives)(int* count);
	void (*script_loaded)(AMX* amx);
	void (*script_unloaded)(AMX* amx);
	bool (*set_codepage)(const char* label, size_t len);
	void (*set_logger)(void* ctx, void (*log)(void* ctx, int level, const char* msg, size_t len));
};

namespace
{

constexpr const char* DEFAULT_CODEPAGE = "windows-1252";

void logLine(void* ctx, int level, const char* msg, size_t len)
{
	static_cast<ICore*>(ctx)->logLn(static_cast<LogLevel>(level), "[pawn-json] %.*s", static_cast<int>(len), msg);
}

class JsonComponent final : public IComponent, public PawnEventHandler
{
	const PjApi& api_;
	ICore* core_ = nullptr;
	IPawnComponent* pawn_ = nullptr;

public:
	PROVIDE_UID(0xb1e7d6813fa9325c);

	explicit JsonComponent(const PjApi& api)
		: api_(api)
	{
	}

	StringView componentName() const override
	{
		return "pawn-json";
	}

	SemanticVersion componentVersion() const override
	{
		return SemanticVersion(PJ_VERSION_MAJOR, PJ_VERSION_MINOR, PJ_VERSION_PATCH, 0);
	}

	void provideConfiguration(ILogger& logger, IEarlyConfig& config, bool defaults) override
	{
		if (defaults || config.getType("json.codepage") == ConfigOptionType_None)
		{
			config.setString("json.codepage", DEFAULT_CODEPAGE);
		}
	}

	void onLoad(ICore* core) override
	{
		core_ = core;
		api_.set_logger(core, &logLine);
		StringView codepage = core->getConfig().getString("json.codepage");
		if (!api_.set_codepage(codepage.data(), codepage.length()))
		{
			core->logLn(LogLevel::Warning, "[pawn-json] json.codepage \"%.*s\" is not a single-byte code page, %s is used", static_cast<int>(codepage.length()), codepage.data(), DEFAULT_CODEPAGE);
		}
	}

	void onInit(IComponentList* components) override
	{
		pawn_ = components->queryComponent<IPawnComponent>();
		if (pawn_)
		{
			pawn_->getEventDispatcher().addEventHandler(this);
		}
		else
		{
			core_->logLn(LogLevel::Error, "[pawn-json] the Pawn component is not loaded, so no script gets the natives");
		}
	}

	void onAmxLoad(IPawnScript& script) override
	{
		int count = 0;
		const AMX_NATIVE_INFO* natives = api_.natives(&count);
		script.Register(natives, count);
		api_.script_loaded(script.GetAMX());
	}

	void onAmxUnload(IPawnScript& script) override
	{
		api_.script_unloaded(script.GetAMX());
	}

	void onFree(IComponent* component) override
	{
		if (component == pawn_)
		{
			pawn_ = nullptr;
		}
	}

	void free() override
	{
		if (pawn_)
		{
			pawn_->getEventDispatcher().removeEventHandler(this);
		}
		delete this;
	}

	void reset() override
	{
	}
};

}

extern "C" IComponent* pj_component(const PjApi* api)
{
	return new JsonComponent(*api);
}
